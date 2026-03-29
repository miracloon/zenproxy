use crate::db::{Database, ProxyQuality};
use dashmap::DashMap;
use rand::seq::SliceRandom;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyStatus {
    Untested,
    Valid,
    Invalid,
}

impl ProxyStatus {
    /// Sort weight: Valid=0, Untested=1, Invalid=2 (lower = higher priority).
    pub fn sort_weight(self) -> u8 {
        match self {
            ProxyStatus::Valid => 0,
            ProxyStatus::Untested => 1,
            ProxyStatus::Invalid => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolProxy {
    pub id: String,
    pub subscription_id: String,
    pub name: String,
    pub proxy_type: String,
    pub server: String,
    pub port: u16,
    pub singbox_outbound: serde_json::Value,
    pub status: ProxyStatus,
    pub local_port: Option<u16>,
    pub error_count: u32,
    pub quality: Option<ProxyQualityInfo>,
    pub is_disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyQualityInfo {
    pub ip_address: Option<String>,
    pub ip_family: Option<String>,
    pub country: Option<String>,
    pub ip_type: Option<String>,
    pub is_residential: bool,
    pub chatgpt_accessible: bool,
    pub google_accessible: bool,
    pub risk_score: f64,
    pub risk_level: String,
    pub checked_at: Option<String>,
    #[serde(skip_serializing)]
    pub incomplete_retry_count: u8,
}

impl From<ProxyQuality> for ProxyQualityInfo {
    fn from(q: ProxyQuality) -> Self {
        let incomplete_retry_count = q
            .extra_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("incomplete_retry_count").and_then(|n| n.as_u64()))
            .map(|n| n.min(u8::MAX as u64) as u8)
            .unwrap_or(0);

        let ip_family = derive_ip_family(q.ip_address.as_deref());

        ProxyQualityInfo {
            ip_address: q.ip_address,
            ip_family,
            country: q.country,
            ip_type: q.ip_type,
            is_residential: q.is_residential,
            chatgpt_accessible: q.chatgpt_accessible,
            google_accessible: q.google_accessible,
            risk_score: q.risk_score,
            risk_level: q.risk_level,
            checked_at: Some(q.checked_at),
            incomplete_retry_count,
        }
    }
}

pub(crate) fn derive_ip_family(ip_address: Option<&str>) -> Option<String> {
    let ip = ip_address?;
    let parsed = ip.parse::<std::net::IpAddr>().ok()?;
    Some(match parsed {
        std::net::IpAddr::V4(_) => "ipv4".to_string(),
        std::net::IpAddr::V6(_) => "ipv6".to_string(),
    })
}

pub struct ProxyPool {
    proxies: DashMap<String, PoolProxy>,
}

impl ProxyPool {
    pub fn new() -> Self {
        ProxyPool {
            proxies: DashMap::new(),
        }
    }

    pub fn load_from_db(&self, db: &Database) {
        let rows = db.get_all_proxies().unwrap_or_default();
        let qualities = db.get_all_qualities().unwrap_or_default();
        let quality_map: std::collections::HashMap<String, ProxyQuality> = qualities
            .into_iter()
            .map(|q| (q.proxy_id.clone(), q))
            .collect();

        for row in rows {
            let quality = quality_map.get(&row.id).map(|q| ProxyQualityInfo::from(q.clone()));
            let outbound: serde_json::Value =
                serde_json::from_str(&row.config_json).unwrap_or_default();
            let status = if row.is_valid {
                ProxyStatus::Valid
            } else if row.last_validated.is_some() {
                ProxyStatus::Invalid
            } else {
                ProxyStatus::Untested
            };
            let proxy = PoolProxy {
                id: row.id.clone(),
                subscription_id: row.subscription_id,
                name: row.name,
                proxy_type: row.proxy_type,
                server: row.server,
                port: row.port as u16,
                singbox_outbound: outbound,
                status,
                local_port: row.local_port.map(|p| p as u16),
                error_count: row.error_count as u32,
                quality,
                is_disabled: row.is_disabled,
            };
            self.proxies.insert(row.id, proxy);
        }
        tracing::info!("Loaded {} proxies into pool", self.proxies.len());
    }

    pub fn add(&self, proxy: PoolProxy) {
        self.proxies.insert(proxy.id.clone(), proxy);
    }

    pub fn remove(&self, id: &str) {
        self.proxies.remove(id);
    }

    pub fn get(&self, id: &str) -> Option<PoolProxy> {
        self.proxies.get(id).map(|p| p.clone())
    }

    pub fn get_all(&self) -> Vec<PoolProxy> {
        self.proxies.iter().map(|p| p.value().clone()).collect()
    }

    pub fn get_valid_proxies(&self) -> Vec<PoolProxy> {
        self.proxies
            .iter()
            .filter(|p| p.status == ProxyStatus::Valid && !p.is_disabled)
            .map(|p| p.value().clone())
            .collect()
    }

    pub fn set_status(&self, id: &str, status: ProxyStatus) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.status = status;
            match status {
                ProxyStatus::Valid => proxy.error_count = 0,
                ProxyStatus::Invalid => proxy.error_count += 1,
                ProxyStatus::Untested => {}
            }
        }
    }

    pub fn set_local_port(&self, id: &str, port: u16) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.local_port = Some(port);
        }
    }

    pub fn clear_local_port(&self, id: &str) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.local_port = None;
        }
    }

    pub fn clear_all_local_ports(&self) {
        for mut proxy in self.proxies.iter_mut() {
            proxy.local_port = None;
        }
    }

    pub fn set_quality(&self, id: &str, quality: ProxyQualityInfo) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.quality = Some(quality);
        }
    }

    pub fn count(&self) -> usize {
        self.proxies.len()
    }

    pub fn count_valid(&self) -> usize {
        self.proxies.iter().filter(|p| p.status == ProxyStatus::Valid && !p.is_disabled).count()
    }

    pub fn set_disabled(&self, id: &str, disabled: bool) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.is_disabled = disabled;
        }
    }

    pub fn remove_by_subscription(&self, sub_id: &str) {
        let ids: Vec<String> = self
            .proxies
            .iter()
            .filter(|p| p.subscription_id == sub_id)
            .map(|p| p.id.clone())
            .collect();
        for id in ids {
            self.proxies.remove(&id);
        }
    }

    pub fn update_proxy_config(&self, id: &str, name: &str, singbox_outbound: serde_json::Value) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.name = name.to_string();
            proxy.singbox_outbound = singbox_outbound;
        }
    }

    pub fn increment_error(&self, id: &str) {
        if let Some(mut proxy) = self.proxies.get_mut(id) {
            proxy.error_count += 1;
        }
    }

    pub fn filter_proxies(&self, filter: &ProxyFilter) -> Vec<PoolProxy> {
        let candidates: Vec<PoolProxy> = self
            .proxies
            .iter()
            .filter(|p| p.status == ProxyStatus::Valid && p.local_port.is_some() && !p.is_disabled)
            .filter(|p| {
                if let Some(ref proxy_type) = filter.proxy_type {
                    p.proxy_type == *proxy_type
                } else {
                    true
                }
            })
            .filter(|p| {
                if filter.chatgpt {
                    p.quality.as_ref().map(|q| q.chatgpt_accessible).unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|p| {
                if filter.google {
                    p.quality.as_ref().map(|q| q.google_accessible).unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|p| {
                if filter.residential {
                    p.quality.as_ref().map(|q| q.is_residential).unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|p| {
                if let Some(max) = filter.risk_max {
                    p.quality.as_ref().map(|q| q.risk_score <= max).unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|p| {
                if let Some(ref country) = filter.country {
                    p.quality
                        .as_ref()
                        .and_then(|q| q.country.as_ref())
                        .map(|c| c.eq_ignore_ascii_case(country))
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .filter(|p| {
                if let Some(ref ip_family) = filter.ip_family {
                    p.quality
                        .as_ref()
                        .and_then(|q| q.ip_family.as_ref())
                        .map(|family| family.eq_ignore_ascii_case(ip_family))
                        .unwrap_or(false)
                } else {
                    true
                }
            })
            .map(|p| p.value().clone())
            .collect();

        candidates
    }

    pub fn pick_random(&self, filter: &ProxyFilter, count: usize) -> Vec<PoolProxy> {
        let mut candidates = self.filter_proxies(filter);
        let mut rng = rand::thread_rng();
        candidates.shuffle(&mut rng);
        candidates.truncate(count);
        candidates
    }
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct ProxyFilter {
    #[serde(default)]
    pub chatgpt: bool,
    #[serde(default)]
    pub google: bool,
    #[serde(default)]
    pub residential: bool,
    pub risk_max: Option<f64>,
    pub country: Option<String>,
    pub ip_family: Option<String>,
    #[serde(rename = "type")]
    pub proxy_type: Option<String>,
    pub count: Option<usize>,
    pub proxy_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ProxyRow;
    use std::path::Path;

    fn sample_pool_proxy(status: ProxyStatus, is_disabled: bool) -> PoolProxy {
        PoolProxy {
            id: "proxy-1".to_string(),
            subscription_id: "sub-1".to_string(),
            name: "Proxy 1".to_string(),
            proxy_type: "vmess".to_string(),
            server: "example.com".to_string(),
            port: 443,
            singbox_outbound: serde_json::json!({}),
            status,
            local_port: None,
            error_count: 0,
            quality: None,
            is_disabled,
        }
    }

    fn sample_proxy_row(
        id: &str,
        is_valid: bool,
        last_validated: Option<&str>,
        is_disabled: bool,
    ) -> ProxyRow {
        ProxyRow {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            name: format!("Proxy {id}"),
            proxy_type: "vmess".to_string(),
            server: "example.com".to_string(),
            port: 443,
            config_json: "{}".to_string(),
            is_valid,
            local_port: None,
            error_count: 0,
            last_error: None,
            last_validated: last_validated.map(str::to_string),
            created_at: "2026-03-28T00:00:00Z".to_string(),
            updated_at: "2026-03-28T00:00:00Z".to_string(),
            is_disabled,
            disabled_at: is_disabled.then(|| "2026-03-28T00:00:00Z".to_string()),
        }
    }

    fn sample_db_quality(proxy_id: &str, ip_address: Option<&str>) -> ProxyQuality {
        ProxyQuality {
            proxy_id: proxy_id.to_string(),
            ip_address: ip_address.map(str::to_string),
            country: Some("US".to_string()),
            ip_type: Some("ISP".to_string()),
            is_residential: false,
            chatgpt_accessible: true,
            google_accessible: true,
            risk_score: 0.1,
            risk_level: "Low".to_string(),
            extra_json: None,
            checked_at: "2026-03-28T00:00:00Z".to_string(),
        }
    }

    fn sample_pool_proxy_with_quality(id: &str, ip_address: &str) -> PoolProxy {
        PoolProxy {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            name: format!("Proxy {id}"),
            proxy_type: "vmess".to_string(),
            server: "example.com".to_string(),
            port: 443,
            singbox_outbound: serde_json::json!({}),
            status: ProxyStatus::Valid,
            local_port: Some(10001),
            error_count: 0,
            quality: Some(ProxyQualityInfo::from(sample_db_quality(id, Some(ip_address)))),
            is_disabled: false,
        }
    }

    #[test]
    fn set_disabled_preserves_existing_validation_status() {
        let pool = ProxyPool::new();
        pool.add(sample_pool_proxy(ProxyStatus::Valid, false));

        pool.set_disabled("proxy-1", true);
        let disabled = pool.get("proxy-1").expect("proxy should exist");
        assert!(disabled.is_disabled);
        assert_eq!(disabled.status, ProxyStatus::Valid);

        pool.set_disabled("proxy-1", false);
        let enabled = pool.get("proxy-1").expect("proxy should exist");
        assert!(!enabled.is_disabled);
        assert_eq!(enabled.status, ProxyStatus::Valid);
    }

    #[test]
    fn load_from_db_keeps_validation_status_for_disabled_rows() {
        let db = Database::new(Path::new(":memory:")).expect("in-memory db should initialize");
        db.insert_proxy(&sample_proxy_row("valid-disabled", true, None, true))
            .expect("valid disabled row should insert");
        db.insert_proxy(&sample_proxy_row(
            "invalid-disabled",
            false,
            Some("2026-03-28T00:00:00Z"),
            true,
        ))
        .expect("invalid disabled row should insert");

        let pool = ProxyPool::new();
        pool.load_from_db(&db);

        let valid_disabled = pool.get("valid-disabled").expect("valid proxy should load");
        assert!(valid_disabled.is_disabled);
        assert_eq!(valid_disabled.status, ProxyStatus::Valid);

        let invalid_disabled = pool
            .get("invalid-disabled")
            .expect("invalid proxy should load");
        assert!(invalid_disabled.is_disabled);
        assert_eq!(invalid_disabled.status, ProxyStatus::Invalid);
    }

    #[test]
    fn proxy_quality_info_derives_ip_family_from_ip_address() {
        let ipv4 = ProxyQualityInfo::from(sample_db_quality("proxy-v4", Some("203.0.113.1")));
        let ipv6 = ProxyQualityInfo::from(sample_db_quality("proxy-v6", Some("2001:db8::1")));

        assert_eq!(ipv4.ip_family.as_deref(), Some("ipv4"));
        assert_eq!(ipv6.ip_family.as_deref(), Some("ipv6"));
    }

    #[test]
    fn filter_proxies_respects_ip_family() {
        let pool = ProxyPool::new();
        pool.add(sample_pool_proxy_with_quality("proxy-v4", "203.0.113.1"));
        pool.add(sample_pool_proxy_with_quality("proxy-v6", "2001:db8::1"));

        let filter = ProxyFilter {
            ip_family: Some("ipv6".to_string()),
            ..Default::default()
        };

        let proxies = pool.filter_proxies(&filter);
        let ids: Vec<_> = proxies.iter().map(|p| p.id.as_str()).collect();

        assert_eq!(ids, vec!["proxy-v6"]);
    }
}
