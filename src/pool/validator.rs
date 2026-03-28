use crate::pool::manager::ProxyStatus;
use crate::AppState;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Two-phase validation:
/// Phase 1: Validate enabled proxies through their existing ports (no port changes)
/// Phase 2: Validate disabled proxies using temporary ports (cleanup after)
pub async fn validate_all(state: Arc<AppState>) -> Result<(), String> {
    let _lock = state.validation_lock.lock().await;

    let total = state.pool.count();
    if total == 0 {
        tracing::info!("No proxies to validate");
        return Ok(());
    }

    let concurrency = state.db.get_setting("validation_concurrency")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.concurrency);
    let timeout_secs = state.db.get_setting("validation_timeout_secs")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.timeout_secs);
    let timeout_duration = std::time::Duration::from_secs(timeout_secs);
    let validation_url = state.db.get_setting("validation_url")
        .ok().flatten()
        .unwrap_or_else(|| state.config.validation.url.clone());

    // --- Phase 1: Validate enabled proxies with existing ports ---
    // Reset Valid proxies with error_count > 0 back to Untested
    let recheck: Vec<String> = state
        .pool
        .get_all()
        .iter()
        .filter(|p| p.status == ProxyStatus::Valid && p.error_count > 0 && !p.is_disabled)
        .map(|p| p.id.clone())
        .collect();
    if !recheck.is_empty() {
        tracing::info!("Re-validating {} proxies with relay errors", recheck.len());
        for id in &recheck {
            state.pool.set_status(id, ProxyStatus::Untested);
        }
    }

    // Ensure enabled proxies have port bindings
    crate::api::subscription::sync_proxy_bindings(&state).await;

    // Collect ALL enabled proxies with ports (regardless of status)
    let enabled_to_validate: Vec<_> = state
        .pool
        .get_all()
        .into_iter()
        .filter(|p| !p.is_disabled && p.local_port.is_some())
        .collect();

    if !enabled_to_validate.is_empty() {
        tracing::info!(
            "Phase 1: validating {} enabled proxies with existing ports",
            enabled_to_validate.len()
        );
        validate_batch(
            &enabled_to_validate,
            &validation_url,
            timeout_duration,
            concurrency,
            &state,
        )
        .await;
    }

    // --- Phase 2: Validate disabled proxies with temporary ports ---
    validate_disabled_proxies(&state, &validation_url, timeout_duration, concurrency).await;

    // Cleanup high-error proxies
    let threshold = state.db.get_setting("validation_error_threshold")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.error_threshold);
    match state.db.cleanup_high_error_proxies(threshold) {
        Ok(count) if count > 0 => {
            tracing::info!("Cleaned up {count} proxies exceeding error threshold");
            let all = state.pool.get_all();
            for p in &all {
                if p.error_count >= threshold {
                    state.pool.remove(&p.id);
                }
            }
        }
        _ => {}
    }

    let valid = state.pool.count_valid();
    let total = state.pool.count();
    tracing::info!("Validation complete: {valid}/{total} valid");

    Ok(())
}

/// Standalone entry point: validate only disabled proxies using temporary ports.
pub async fn validate_disabled_only(state: Arc<AppState>) -> Result<(), String> {
    let _lock = state.validation_lock.lock().await;

    let concurrency = state.db.get_setting("validation_concurrency")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.concurrency);
    let timeout_secs = state.db.get_setting("validation_timeout_secs")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.timeout_secs);
    let timeout_duration = std::time::Duration::from_secs(timeout_secs);
    let validation_url = state.db.get_setting("validation_url")
        .ok().flatten()
        .unwrap_or_else(|| state.config.validation.url.clone());

    validate_disabled_proxies(&state, &validation_url, timeout_duration, concurrency).await;
    Ok(())
}

/// Validate disabled proxies using temporary ports.
/// For each batch:
/// 1. Create temporary bindings (prefer remembered port if available)
/// 2. Validate through temporary ports
/// 3. Update status (Valid/Invalid)
/// 4. Destroy temporary bindings — DO NOT write to DB local_port
pub async fn validate_disabled_proxies(
    state: &Arc<AppState>,
    validation_url: &str,
    timeout: std::time::Duration,
    concurrency: usize,
) {
    let batch_size = state.db.get_setting("validation_batch_size")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.batch_size);

    let disabled: Vec<_> = state
        .pool
        .get_all()
        .into_iter()
        .filter(|p| p.is_disabled)
        .collect();

    if disabled.is_empty() {
        return;
    }

    tracing::info!(
        "Phase 2: validating {} disabled proxies in batches of {}",
        disabled.len(), batch_size
    );

    for (batch_idx, batch) in disabled.chunks(batch_size).enumerate() {
        // Create temp bindings
        let mut temp_assignments: Vec<(crate::pool::manager::PoolProxy, u16)> = Vec::new();
        {
            let mut mgr = state.singbox.lock().await;
            for proxy in batch {
                // Check if proxy has a remembered port in DB
                let db_port = proxy.local_port; // from pool, which was loaded from DB
                // If pool doesn't have it, check DB directly (pool may have cleared it)
                let remembered_port = if db_port.is_some() {
                    db_port
                } else {
                    // Read from DB directly — the proxy may have port memory
                    state.db.get_all_proxies().ok()
                        .and_then(|rows| rows.into_iter()
                            .find(|r| r.id == proxy.id)
                            .and_then(|r| r.local_port.map(|p| p as u16)))
                };

                let result = if let Some(port) = remembered_port {
                    mgr.create_binding_on_port(&proxy.id, port, &proxy.singbox_outbound).await
                } else {
                    mgr.create_binding(&proxy.id, &proxy.singbox_outbound).await
                };

                match result {
                    Ok(port) => {
                        temp_assignments.push((proxy.clone(), port));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create temp binding for {}: {e}", proxy.name);
                    }
                }
            }
        }

        if temp_assignments.is_empty() {
            continue;
        }

        // Set temp local_port in pool for validation
        for (proxy, port) in &temp_assignments {
            state.pool.set_local_port(&proxy.id, *port);
        }

        // Build validation targets with temp ports
        let to_validate: Vec<_> = temp_assignments.iter()
            .filter_map(|(proxy, port)| {
                let mut p = proxy.clone();
                p.local_port = Some(*port);
                Some(p)
            })
            .collect();

        tracing::info!(
            "Phase 2 batch {}: validating {} disabled proxies",
            batch_idx + 1, to_validate.len()
        );

        validate_batch(&to_validate, validation_url, timeout, concurrency, state).await;

        // Cleanup: remove temp bindings, restore pool state
        {
            let mut mgr = state.singbox.lock().await;
            for (proxy, port) in &temp_assignments {
                mgr.remove_binding(&proxy.id, *port).await.ok();
                state.pool.clear_local_port(&proxy.id);
                // DO NOT write to DB — temp ports don't create memory
            }
        }
    }
}

/// Validate a batch of proxies concurrently, reusing one reqwest::Client per proxy port.
async fn validate_batch(
    proxies: &[crate::pool::manager::PoolProxy],
    validation_url: &str,
    timeout: std::time::Duration,
    concurrency: usize,
    state: &Arc<AppState>,
) -> usize {
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(proxies.len());

    for proxy in proxies {
        let local_port = match proxy.local_port {
            Some(p) => p,
            None => continue,
        };

        let sem = semaphore.clone();
        let state = state.clone();
        let url = validation_url.to_string();
        let proxy_id = proxy.id.clone();
        let proxy_name = proxy.name.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let proxy_addr = format!("http://127.0.0.1:{local_port}");
            let result = validate_single(&proxy_addr, &url, timeout).await;

            match result {
                Ok(()) => {
                    state.pool.set_status(&proxy_id, ProxyStatus::Valid);
                    state
                        .db
                        .update_proxy_validation(&proxy_id, true, None)
                        .ok();
                }
                Err(e) => {
                    tracing::debug!("Proxy {proxy_name} failed validation: {e}");
                    state.pool.set_status(&proxy_id, ProxyStatus::Invalid);
                    state
                        .db
                        .update_proxy_validation(&proxy_id, false, Some(&e))
                        .ok();
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

async fn validate_single(
    proxy_addr: &str,
    target_url: &str,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let proxy = reqwest::Proxy::all(proxy_addr).map_err(|e| format!("Proxy config error: {e}"))?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(proxy)
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .pool_max_idle_per_host(0) // don't keep idle connections
        .build()
        .map_err(|e| format!("Client build error: {e}"))?;

    let resp = client
        .get(target_url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() || resp.status().is_redirection() {
        Ok(())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}
