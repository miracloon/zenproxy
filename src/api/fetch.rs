use crate::api::auth;
use crate::error::AppError;
use crate::pool::manager::{ProxyFilter, ProxyStatus};
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct FetchQuery {
    pub api_key: Option<String>,
    #[serde(default)]
    pub all: bool,
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

pub async fn fetch_proxies(
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
        ip_family: query.ip_family,
        proxy_type: query.proxy_type,
        count: query.count,
        proxy_id: query.proxy_id,
    };
    let count = filter.count.unwrap_or(1);

    if let Some(ref id) = filter.proxy_id {
        if let Some(proxy) = state.pool.get(id) {
            return Ok(Json(json!({
                "proxies": [proxy_to_json(&proxy)]
            })));
        } else {
            return Err(AppError::NotFound(format!("Proxy {id} not found")));
        }
    }

    let proxies = state.pool.pick_random(&filter, count);
    if proxies.is_empty() {
        return Ok(Json(json!({
            "proxies": [],
            "message": "No proxies match the given filters"
        })));
    }

    let proxy_list: Vec<serde_json::Value> = proxies.iter().map(proxy_to_json).collect();

    Ok(Json(json!({
        "proxies": proxy_list,
        "count": proxy_list.len(),
    })))
}

/// User-accessible proxy list with full quality details
pub async fn list_all_proxies(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ApiKeyQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth::authenticate_request(&state, &headers, query.api_key.as_deref()).await?;

    let proxies = state.pool.get_all();
    let total = proxies.len();
    let valid = proxies.iter().filter(|p| p.status == ProxyStatus::Valid).count();
    let untested = proxies.iter().filter(|p| p.status == ProxyStatus::Untested).count();
    let invalid = proxies.iter().filter(|p| p.status == ProxyStatus::Invalid).count();
    let quality_checked = proxies.iter().filter(|p| p.quality.is_some()).count();
    let chatgpt_count = proxies.iter().filter(|p| p.quality.as_ref().map_or(false, |q| q.chatgpt_accessible)).count();
    let google_count = proxies.iter().filter(|p| p.quality.as_ref().map_or(false, |q| q.google_accessible)).count();
    let residential_count = proxies.iter().filter(|p| p.quality.as_ref().map_or(false, |q| q.is_residential)).count();

    let proxy_list: Vec<serde_json::Value> = proxies.iter().map(proxy_to_json).collect();

    Ok(Json(json!({
        "proxies": proxy_list,
        "total": total,
        "valid": valid,
        "untested": untested,
        "invalid": invalid,
        "quality_checked": quality_checked,
        "chatgpt_accessible": chatgpt_count,
        "google_accessible": google_count,
        "residential": residential_count,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ApiKeyQuery {
    pub api_key: Option<String>,
}

fn proxy_to_json(p: &crate::pool::manager::PoolProxy) -> serde_json::Value {
    json!({
        "id": p.id,
        "name": p.name,
        "type": p.proxy_type,
        "server": p.server,
        "port": p.port,
        "local_port": p.local_port,
        "status": p.status,
        "error_count": p.error_count,
        "quality": p.quality.as_ref().map(|q| json!({
            "ip_address": q.ip_address,
            "ip_family": q.ip_family,
            "country": q.country,
            "ip_type": q.ip_type,
            "is_residential": q.is_residential,
            "chatgpt": q.chatgpt_accessible,
            "google": q.google_accessible,
            "risk_score": q.risk_score,
            "risk_level": q.risk_level,
        })),
    })
}
