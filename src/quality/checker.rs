use crate::db::ProxyQuality;
use crate::pool::manager::{PoolProxy, ProxyQualityInfo, derive_ip_family};
use crate::AppState;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::Instant;

/// Staleness threshold: re-check quality after 24 hours.
const STALE_HOURS: i64 = 24;
/// Incomplete quality data can be retried at most this many times.
const MAX_INCOMPLETE_RETRIES: u8 = 2;
/// Limit checks per run so quality task won't hold validation resources for too long.
const MAX_QUALITY_CHECKS_PER_RUN: usize = 40;

/// ip-api.com rate limiter: max 40 requests/minute (free tier limit is 45).
pub(crate) struct RateLimiter {
    last_call: Mutex<Instant>,
    min_interval: std::time::Duration,
}

impl RateLimiter {
    pub(crate) fn new(calls_per_minute: u32) -> Self {
        RateLimiter {
            last_call: Mutex::new(Instant::now() - std::time::Duration::from_secs(60)),
            min_interval: std::time::Duration::from_millis(60_000 / calls_per_minute as u64),
        }
    }

    pub(crate) async fn wait(&self) {
        let mut last = self.last_call.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.min_interval {
            tokio::time::sleep(self.min_interval - elapsed).await;
        }
        *last = Instant::now();
    }
}

/// Returns the number of proxies actually checked.
pub async fn check_all(state: Arc<AppState>) -> Result<usize, String> {
    let now = chrono::Utc::now();
    let mut total_checked = 0usize;
    let rate_limiter = Arc::new(RateLimiter::new(40));
    let mut remaining_budget = MAX_QUALITY_CHECKS_PER_RUN;

    // Quality-check all valid proxies that already have an active port.
    let mut to_check: Vec<PoolProxy> = state
        .pool
        .get_all()
        .into_iter()
        .filter(|p| p.status == crate::pool::manager::ProxyStatus::Valid && p.local_port.is_some())
        .filter(|p| needs_quality_check(p, &now))
        .collect();

    if !to_check.is_empty() {
        if to_check.len() > remaining_budget {
            to_check.truncate(remaining_budget);
        }
        tracing::info!(
            "Quality check: checking {} proxies this run (limit={MAX_QUALITY_CHECKS_PER_RUN})",
            to_check.len()
        );
        total_checked += check_batch(&to_check, &state, &rate_limiter).await;
        remaining_budget = remaining_budget.saturating_sub(to_check.len());
    }

    if remaining_budget > 0 {
        let mut disabled_valid: Vec<PoolProxy> = state
            .pool
            .get_all()
            .into_iter()
            .filter(|p| {
                p.is_disabled
                    && p.status == crate::pool::manager::ProxyStatus::Valid
                    && p.local_port.is_none()
            })
            .filter(|p| needs_quality_check(p, &now))
            .collect();

        if disabled_valid.len() > remaining_budget {
            disabled_valid.truncate(remaining_budget);
        }

        if !disabled_valid.is_empty() {
            let batch_size = 10;
            for batch in disabled_valid.chunks(batch_size) {
                let mut temp_assignments: Vec<(PoolProxy, u16)> = Vec::new();
                {
                    let mut mgr = state.singbox.lock().await;
                    for proxy in batch {
                        let remembered_port = proxy.local_port.or_else(|| {
                            state.db.get_all_proxies().ok().and_then(|rows| {
                                rows.into_iter()
                                    .find(|row| row.id == proxy.id)
                                    .and_then(|row| row.local_port.map(|port| port as u16))
                            })
                        });

                        let result = if let Some(port) = remembered_port {
                            mgr.create_binding_on_port(&proxy.id, port, &proxy.singbox_outbound)
                                .await
                        } else {
                            mgr.create_binding(&proxy.id, &proxy.singbox_outbound).await
                        };

                        match result {
                            Ok(port) => temp_assignments.push((proxy.clone(), port)),
                            Err(e) => {
                                tracing::warn!(
                                    "Quality check temp binding failed for {}: {e}",
                                    proxy.name
                                );
                            }
                        }
                    }
                }

                if temp_assignments.is_empty() {
                    continue;
                }

                for (proxy, port) in &temp_assignments {
                    state.pool.set_local_port(&proxy.id, *port);
                }

                let to_check_with_ports: Vec<_> = temp_assignments
                    .iter()
                    .map(|(proxy, port)| {
                        let mut proxy = proxy.clone();
                        proxy.local_port = Some(*port);
                        proxy
                    })
                    .collect();
                total_checked += check_batch(&to_check_with_ports, &state, &rate_limiter).await;

                let mut mgr = state.singbox.lock().await;
                for (proxy, port) in &temp_assignments {
                    mgr.remove_binding(&proxy.id, *port).await.ok();
                    state.pool.clear_local_port(&proxy.id);
                }
            }
        }
    }

    if total_checked > 0 {
        tracing::info!("Quality check complete: {total_checked} proxies checked in this run");
    }

    Ok(total_checked)
}

/// Run quality check on a single proxy by ID. The proxy must have an active binding.
pub(crate) async fn check_single_proxy(state: &Arc<AppState>, proxy_id: &str) -> Result<(), String> {
    let proxy = state.pool.get(proxy_id)
        .ok_or_else(|| "Proxy not found".to_string())?;

    if proxy.local_port.is_none() {
        return Err("Proxy has no active binding".into());
    }

    let local_port = proxy.local_port.unwrap();
    let proxy_addr = format!("http://127.0.0.1:{local_port}");
    let rate_limiter = RateLimiter::new(40);

    match check_single(&proxy_addr, &proxy, &rate_limiter).await {
        Ok(quality) => {
            if all_quality_probes_failed(&quality) {
                let reason = "Quality check: all probes failed, proxy likely unreachable";
                state
                    .pool
                    .set_status(&proxy.id, crate::pool::manager::ProxyStatus::Invalid);
                state
                    .db
                    .update_proxy_validation(&proxy.id, false, Some(reason))
                    .ok();
                tracing::warn!("Single quality check all probes failed for {}", proxy.id);
                return Err("All quality probes failed, proxy likely unreachable".into());
            }

            let db_quality = ProxyQuality {
                proxy_id: proxy.id.clone(),
                ip_address: quality.ip_address.clone(),
                country: quality.country.clone(),
                ip_type: quality.ip_type.clone(),
                is_residential: quality.is_residential,
                chatgpt_accessible: quality.chatgpt_accessible,
                google_accessible: quality.google_accessible,
                risk_score: quality.risk_score,
                risk_level: quality.risk_level.clone(),
                extra_json: Some(serde_json::json!({"incomplete_retry_count": 0}).to_string()),
                checked_at: chrono::Utc::now().to_rfc3339(),
            };
            state.db.upsert_quality(&db_quality).ok();
            state.pool.set_quality(&proxy.id, quality);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Check a batch of proxies concurrently, respecting rate limits.
async fn check_batch(
    proxies: &[PoolProxy],
    state: &Arc<AppState>,
    rate_limiter: &Arc<RateLimiter>,
) -> usize {
    let quality_concurrency = state.db.get_setting("quality_concurrency")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.quality.concurrency);
    let semaphore = Arc::new(Semaphore::new(quality_concurrency));
    let mut handles = Vec::new();

    for proxy in proxies.iter().cloned() {
        let sem = semaphore.clone();
        let state = state.clone();
        let rl = rate_limiter.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let local_port = match proxy.local_port {
                Some(p) => p,
                None => return,
            };

            let proxy_addr = format!("http://127.0.0.1:{local_port}");
            match check_single(&proxy_addr, &proxy, &rl).await {
                Ok(quality) => {
                    if all_quality_probes_failed(&quality) {
                        let reason = "Quality check: all probes failed, proxy likely unreachable";
                        state
                            .pool
                            .set_status(&proxy.id, crate::pool::manager::ProxyStatus::Invalid);
                        state
                            .db
                            .update_proxy_validation(&proxy.id, false, Some(reason))
                            .ok();
                        tracing::warn!(
                            "All quality probes failed for {}, marking Invalid",
                            proxy.name
                        );
                        return;
                    }

                    let is_incomplete = quality_is_incomplete(&quality);
                    let incomplete_retry_count = if is_incomplete {
                        proxy
                            .quality
                            .as_ref()
                            .map(|q| q.incomplete_retry_count)
                            .unwrap_or(0)
                            .saturating_add(1)
                    } else {
                        0
                    };

                    tracing::info!(
                        "Quality OK: {} | IP={} country={} type={} residential={} google={} chatgpt={} risk={}({})",
                        proxy.name,
                        quality.ip_address.as_deref().unwrap_or("-"),
                        quality.country.as_deref().unwrap_or("-"),
                        quality.ip_type.as_deref().unwrap_or("-"),
                        quality.is_residential,
                        quality.google_accessible,
                        quality.chatgpt_accessible,
                        quality.risk_score,
                        &quality.risk_level,
                    );
                    let db_quality = ProxyQuality {
                        proxy_id: proxy.id.clone(),
                        ip_address: quality.ip_address.clone(),
                        country: quality.country.clone(),
                        ip_type: quality.ip_type.clone(),
                        is_residential: quality.is_residential,
                        chatgpt_accessible: quality.chatgpt_accessible,
                        google_accessible: quality.google_accessible,
                        risk_score: quality.risk_score,
                        risk_level: quality.risk_level.clone(),
                        extra_json: Some(
                            serde_json::json!({
                                "incomplete_retry_count": incomplete_retry_count,
                            })
                            .to_string(),
                        ),
                        checked_at: chrono::Utc::now().to_rfc3339(),
                    };
                    state.db.upsert_quality(&db_quality).ok();
                    let mut quality_to_pool = quality;
                    quality_to_pool.incomplete_retry_count = incomplete_retry_count;
                    state.pool.set_quality(&proxy.id, quality_to_pool);
                }
                Err(e) => {
                    tracing::warn!("Quality check failed for {}: {e}", proxy.name);
                }
            }
        });
        handles.push(handle);
    }

    let mut count = 0;
    for handle in handles {
        if handle.await.is_ok() {
            count += 1;
        }
    }
    count
}

/// Check if a proxy needs a quality check: no quality data, incomplete data, or stale.
fn needs_quality_check(proxy: &PoolProxy, now: &chrono::DateTime<chrono::Utc>) -> bool {
    match &proxy.quality {
        None => true,
        Some(q) => {
            // Incomplete data → retry
            if quality_is_incomplete(q) {
                if q.incomplete_retry_count >= MAX_INCOMPLETE_RETRIES {
                    return false;
                }
                return true;
            }
            match &q.checked_at {
                None => true,
                Some(checked_at) => {
                    match chrono::DateTime::parse_from_rfc3339(checked_at) {
                        Ok(t) => {
                            let age = *now - t.with_timezone(&chrono::Utc);
                            age.num_hours() >= STALE_HOURS
                        }
                        Err(_) => true, // unparseable → re-check
                    }
                }
            }
        }
    }
}

fn quality_is_incomplete(q: &ProxyQualityInfo) -> bool {
    q.country.is_none() || q.ip_type.is_none() || q.ip_address.is_none() || q.risk_level == "Unknown"
}

fn all_quality_probes_failed(q: &ProxyQualityInfo) -> bool {
    q.ip_address.is_none() && !q.google_accessible && !q.chatgpt_accessible
}

/// IP info from ip-api.com (primary source — free, no key, auto-detects caller IP)
struct IpApiResult {
    ip: Option<String>,
    country: Option<String>,
    is_proxy: bool,
    is_hosting: bool,
}

async fn check_single(
    proxy_addr: &str,
    _proxy: &PoolProxy,
    rate_limiter: &RateLimiter,
) -> Result<ProxyQualityInfo, String> {
    let proxy = reqwest::Proxy::all(proxy_addr).map_err(|e| e.to_string())?;
    // no_proxy() must come BEFORE .proxy() — it clears all proxies and disables
    // env var detection; the subsequent .proxy() then adds our explicit proxy back.
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(30))
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;

    // Rate-limit ip-api.com calls, run other checks in parallel
    let (ipapi_result, ipinfo_result, google_accessible, chatgpt_accessible) = tokio::join!(
        rate_limited_ip_api(&client, rate_limiter),
        query_ipinfo(&client),
        check_google(&client),
        check_chatgpt(&client),
    );

    // Merge IP info: prefer ipinfo.io for org detail, fall back to ip-api.com for IP/country
    let (ip_address, country, ip_type, mut is_residential) = match ipinfo_result {
        Some((ip, country, ip_type, residential)) => (ip, country, ip_type, residential),
        None => {
            // ipinfo.io failed — use ip-api.com as fallback
            let ip = ipapi_result.as_ref().and_then(|r| r.ip.clone());
            let country = ipapi_result.as_ref().and_then(|r| r.country.clone());
            (ip, country, None, false)
        }
    };

    // Risk scoring from ip-api.com
    let (risk_score, risk_level, is_hosting) = match &ipapi_result {
        Some(r) => {
            let (score, level) = match (r.is_proxy, r.is_hosting) {
                (true, true) => (0.9, "Very High"),
                (true, false) => (0.7, "High"),
                (false, true) => (0.5, "Medium"),
                (false, false) => (0.1, "Low"),
            };
            (score, level.to_string(), r.is_hosting)
        }
        None => (0.5, "Unknown".to_string(), false),
    };

    // ip-api.com hosting flag overrides residential detection
    let ip_type = if is_hosting {
        is_residential = false;
        Some("Datacenter".to_string())
    } else {
        ip_type
    };

    let ip_family = derive_ip_family(ip_address.as_deref());

    Ok(ProxyQualityInfo {
        ip_address,
        ip_family,
        country,
        ip_type,
        is_residential,
        chatgpt_accessible,
        google_accessible,
        risk_score,
        risk_level,
        checked_at: Some(chrono::Utc::now().to_rfc3339()),
        incomplete_retry_count: 0,
    })
}

/// Wraps query_ip_api with rate limiting to stay under free tier limits.
async fn rate_limited_ip_api(
    client: &reqwest::Client,
    rate_limiter: &RateLimiter,
) -> Option<IpApiResult> {
    rate_limiter.wait().await;
    query_ip_api(client).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_quality(
        ip_address: Option<&str>,
        google_accessible: bool,
        chatgpt_accessible: bool,
    ) -> ProxyQualityInfo {
        ProxyQualityInfo {
            ip_address: ip_address.map(str::to_string),
            ip_family: derive_ip_family(ip_address),
            country: None,
            ip_type: None,
            is_residential: false,
            chatgpt_accessible,
            google_accessible,
            risk_score: 0.0,
            risk_level: "Unknown".to_string(),
            checked_at: None,
            incomplete_retry_count: 0,
        }
    }

    #[test]
    fn all_quality_probes_failed_only_when_every_probe_is_empty() {
        assert!(all_quality_probes_failed(&sample_quality(None, false, false)));
        assert!(!all_quality_probes_failed(&sample_quality(Some("1.1.1.1"), false, false)));
        assert!(!all_quality_probes_failed(&sample_quality(None, true, false)));
        assert!(!all_quality_probes_failed(&sample_quality(None, false, true)));
    }
}

/// Query ip-api.com — auto-detects caller IP, returns IP/country/proxy/hosting.
/// Retries up to 2 times on failure.
async fn query_ip_api(client: &reqwest::Client) -> Option<IpApiResult> {
    let url = "http://ip-api.com/json?fields=query,countryCode,proxy,hosting,status,message";
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        let resp = match client.get(url).send().await {
            Ok(r) if r.status().as_u16() == 429 => {
                tracing::warn!("ip-api.com rate limited (attempt {}), backing off", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!("ip-api.com returned status {} (attempt {})", r.status(), attempt + 1);
                continue;
            }
            Err(e) => {
                tracing::warn!("ip-api.com request failed (attempt {}): {e}", attempt + 1);
                continue;
            }
        };
        match resp.json::<serde_json::Value>().await {
            Ok(body) if body["status"].as_str() == Some("success") => {
                return Some(IpApiResult {
                    ip: body["query"].as_str().map(|s| s.to_string()),
                    country: body["countryCode"].as_str().map(|s| s.to_string()),
                    is_proxy: body["proxy"].as_bool().unwrap_or(false),
                    is_hosting: body["hosting"].as_bool().unwrap_or(false),
                });
            }
            Ok(body) => {
                tracing::warn!(
                    "ip-api.com returned non-success: {}",
                    body["message"].as_str().unwrap_or("unknown")
                );
                return None; // API-level failure, don't retry
            }
            Err(e) => {
                tracing::warn!("ip-api.com parse failed (attempt {}): {e}", attempt + 1);
            }
        }
    }
    None
}

/// Query ipinfo.io — richer org/company data for residential detection.
/// Retries up to 2 times on failure.
async fn query_ipinfo(
    client: &reqwest::Client,
) -> Option<(Option<String>, Option<String>, Option<String>, bool)> {
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        let resp = match client.get("https://ipinfo.io/json").send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!("ipinfo.io returned status {} (attempt {})", r.status(), attempt + 1);
                continue;
            }
            Err(e) => {
                tracing::warn!("ipinfo.io request failed (attempt {}): {e}", attempt + 1);
                continue;
            }
        };
        match resp.json::<serde_json::Value>().await {
            Ok(body) => {
                let ip = body["ip"].as_str().map(|s| s.to_string());
                let country = body["country"].as_str().map(|s| s.to_string());
                let org = body["org"].as_str().unwrap_or("");
                let org_lower = org.to_lowercase();

                let company_type = body["company"]["type"].as_str().unwrap_or("");

                let (ip_type, is_residential) = if !company_type.is_empty() {
                    let residential = company_type.eq_ignore_ascii_case("isp");
                    (Some(company_type.to_string()), residential)
                } else {
                    let is_datacenter = org_lower.contains("hosting")
                        || org_lower.contains("cloud")
                        || org_lower.contains("server")
                        || org_lower.contains("data center")
                        || org_lower.contains("datacenter")
                        || org_lower.contains("vps")
                        || org_lower.contains("amazon")
                        || org_lower.contains("google")
                        || org_lower.contains("microsoft")
                        || org_lower.contains("digitalocean")
                        || org_lower.contains("linode")
                        || org_lower.contains("vultr")
                        || org_lower.contains("hetzner")
                        || org_lower.contains("ovh")
                        || org_lower.contains("contabo")
                        || org_lower.contains("alibaba")
                        || org_lower.contains("tencent")
                        || org_lower.contains("oracle");

                    if is_datacenter {
                        (Some("Datacenter".to_string()), false)
                    } else {
                        (Some("ISP".to_string()), true)
                    }
                };

                return Some((ip, country, ip_type, is_residential));
            }
            Err(e) => {
                tracing::warn!("ipinfo.io parse failed (attempt {}): {e}", attempt + 1);
            }
        }
    }
    None
}

async fn check_google(client: &reqwest::Client) -> bool {
    match client
        .get("https://www.google.com/generate_204")
        .send()
        .await
    {
        Ok(r) => r.status().as_u16() == 204 || r.status().is_success(),
        Err(_) => false,
    }
}

async fn check_chatgpt(client: &reqwest::Client) -> bool {
    match client.get("https://chatgpt.com/").send().await {
        Ok(r) => {
            let status = r.status();
            if status == reqwest::StatusCode::FORBIDDEN {
                return false;
            }
            if !status.is_success() && !status.is_redirection() {
                return false;
            }
            match r.text().await {
                Ok(body) => {
                    !body.contains("unsupported_country")
                        && !body.contains("unavailable in your country")
                        && !body.contains("not available")
                }
                Err(_) => true,
            }
        }
        Err(_) => false,
    }
}
