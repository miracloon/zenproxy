use crate::api::auth;
use crate::api::fetch::FetchQuery;
use crate::error::AppError;
use crate::pool::manager::ProxyFilter;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

fn select_client_fetch_proxies(
    pool: &crate::pool::manager::ProxyPool,
    filter: &ProxyFilter,
    all: bool,
) -> Vec<crate::pool::manager::PoolProxy> {
    if all {
        pool.filter_proxies(filter)
    } else {
        let count = filter.count.unwrap_or(10);
        pool.pick_random(filter, count)
    }
}

/// Client fetch endpoint - returns proxies with their outbound configurations.
/// Used by local sing-box clients to get proxy configs for direct use.
pub async fn client_fetch_proxies(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<FetchQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth::authenticate_request(&state, &headers, query.api_key.as_deref()).await?;

    let filter = ProxyFilter {
        chatgpt: query.chatgpt,
        google: query.google,
        residential: query.residential,
        risk_max: query.risk_max,
        country: query.country,
        proxy_type: query.proxy_type,
        count: query.count,
        proxy_id: query.proxy_id,
    };

    if let Some(ref id) = filter.proxy_id {
        if let Some(proxy) = state.pool.get(id) {
            return Ok(Json(json!({
                "proxies": [client_proxy_to_json(&proxy)],
                "count": 1
            })));
        } else {
            return Err(AppError::NotFound(format!("Proxy {id} not found")));
        }
    }

    let proxies = select_client_fetch_proxies(&state.pool, &filter, query.all);
    if proxies.is_empty() {
        return Ok(Json(json!({
            "proxies": [],
            "count": 0,
            "message": "No proxies match the given filters"
        })));
    }

    let proxy_list: Vec<serde_json::Value> = proxies.iter().map(client_proxy_to_json).collect();
    let len = proxy_list.len();

    Ok(Json(json!({
        "proxies": proxy_list,
        "count": len,
    })))
}

fn client_proxy_to_json(p: &crate::pool::manager::PoolProxy) -> serde_json::Value {
    json!({
        "id": p.id,
        "name": p.name,
        "type": p.proxy_type,
        "server": p.server,
        "port": p.port,
        "outbound": p.singbox_outbound,
        "local_port": p.local_port,
        "quality": p.quality.as_ref().map(|q| json!({
            "country": q.country,
            "chatgpt": q.chatgpt_accessible,
            "google": q.google_accessible,
            "is_residential": q.is_residential,
            "risk_score": q.risk_score,
            "risk_level": q.risk_level,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::manager::{PoolProxy, ProxyPool, ProxyQualityInfo, ProxyStatus};
    use serde_json::json;

    fn sample_proxy(
        id: &str,
        status: ProxyStatus,
        is_disabled: bool,
        local_port: Option<u16>,
    ) -> PoolProxy {
        PoolProxy {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            name: format!("proxy-{id}"),
            proxy_type: "vmess".to_string(),
            server: format!("{id}.example.com"),
            port: 443,
            singbox_outbound: json!({
                "type": "vmess",
                "server": format!("{id}.example.com"),
                "server_port": 443
            }),
            status,
            local_port,
            error_count: 0,
            quality: Some(ProxyQualityInfo {
                ip_address: Some("203.0.113.1".to_string()),
                country: Some("US".to_string()),
                ip_type: Some("ISP".to_string()),
                is_residential: false,
                chatgpt_accessible: true,
                google_accessible: true,
                risk_score: 0.1,
                risk_level: "Low".to_string(),
                checked_at: Some("2026-03-29T00:00:00Z".to_string()),
                incomplete_retry_count: 0,
            }),
            is_disabled,
        }
    }

    fn sample_pool() -> ProxyPool {
        let pool = ProxyPool::new();
        pool.add(sample_proxy("valid-a", ProxyStatus::Valid, false, Some(10001)));
        pool.add(sample_proxy("valid-b", ProxyStatus::Valid, false, Some(10002)));
        pool.add(sample_proxy("disabled", ProxyStatus::Valid, true, Some(10003)));
        pool.add(sample_proxy("untested", ProxyStatus::Untested, false, Some(10004)));
        pool.add(sample_proxy("invalid", ProxyStatus::Invalid, false, Some(10005)));
        pool.add(sample_proxy("no-port", ProxyStatus::Valid, false, None));
        pool
    }

    #[test]
    fn all_mode_returns_all_eligible_client_fetch_proxies() {
        let pool = sample_pool();
        let filter = ProxyFilter {
            count: Some(1),
            ..Default::default()
        };

        let proxies = select_client_fetch_proxies(&pool, &filter, true);

        let ids: Vec<_> = proxies.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"valid-a"));
        assert!(ids.contains(&"valid-b"));
    }

    #[test]
    fn all_mode_ignores_count_limit() {
        let pool = sample_pool();
        let filter = ProxyFilter {
            count: Some(1),
            ..Default::default()
        };

        let proxies = select_client_fetch_proxies(&pool, &filter, true);

        assert_eq!(proxies.len(), 2);
    }

    #[test]
    fn random_mode_still_honors_count_limit() {
        let pool = sample_pool();
        let filter = ProxyFilter {
            count: Some(1),
            ..Default::default()
        };

        let proxies = select_client_fetch_proxies(&pool, &filter, false);

        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].status, ProxyStatus::Valid);
        assert!(!proxies[0].is_disabled);
        assert!(proxies[0].local_port.is_some());
    }
}
