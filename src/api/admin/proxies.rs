use crate::error::AppError;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

pub async fn list_proxies(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxies = state.pool.get_all();
    let proxy_list: Vec<serde_json::Value> = proxies
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "subscription_id": p.subscription_id,
                "name": p.name,
                "type": p.proxy_type,
                "server": p.server,
                "port": p.port,
                "local_port": p.local_port,
                "status": p.status,
                "error_count": p.error_count,
                "is_disabled": p.is_disabled,
                "quality": p.quality.as_ref().map(|q| json!({
                    "ip_address": q.ip_address,
                    "country": q.country,
                    "ip_type": q.ip_type,
                    "is_residential": q.is_residential,
                    "chatgpt": q.chatgpt_accessible,
                    "google": q.google_accessible,
                    "risk_score": q.risk_score,
                    "risk_level": q.risk_level,
                })),
            })
        })
        .collect();

    Ok(Json(json!({
        "proxies": proxy_list,
        "total": proxy_list.len(),
    })))
}

pub async fn delete_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.pool.remove(&id);
    state.db.delete_proxy(&id)?;
    Ok(Json(json!({ "message": "Proxy deleted" })))
}

pub async fn cleanup_proxies(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let threshold = state.db.get_setting("validation_error_threshold")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(state.config.validation.error_threshold);
    let count = state.db.cleanup_high_error_proxies(threshold)?;

    // Remove from pool too
    let all = state.pool.get_all();
    for p in &all {
        if p.error_count >= threshold {
            state.pool.remove(&p.id);
        }
    }

    Ok(Json(json!({
        "message": format!("Cleaned up {count} proxies"),
        "removed": count,
    })))
}

pub async fn trigger_validation(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::pool::validator::validate_all(state_clone).await {
            tracing::error!("Manual validation failed: {e}");
        }
    });

    Ok(Json(json!({
        "message": "Validation started in background"
    })))
}

pub async fn trigger_quality_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::quality::checker::check_all(state_clone).await {
            tracing::error!("Manual quality check failed: {e}");
        }
    });

    Ok(Json(json!({
        "message": "Quality check started in background"
    })))
}

pub async fn toggle_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    let new_disabled = !proxy.is_disabled;
    state.pool.set_disabled(&id, new_disabled);
    state.db.set_proxy_disabled(&id, new_disabled)?;

    // If disabling, clear the local port binding
    if new_disabled {
        if let Some(port) = proxy.local_port {
            let mut mgr = state.singbox.lock().await;
            mgr.remove_binding(&id, port).await.ok();
            state.pool.clear_local_port(&id);
            state.db.update_proxy_local_port_null(&id).ok();
        }
    }

    let status_str = if new_disabled { "disabled" } else { "enabled" };
    tracing::info!("Proxy {} {} (name={})", id, status_str, proxy.name);
    Ok(Json(json!({
        "message": format!("Proxy {}", status_str),
        "is_disabled": new_disabled,
    })))
}

pub async fn validate_single_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    if proxy.is_disabled {
        return Err(AppError::BadRequest("Cannot validate a disabled proxy".into()));
    }

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        let validation_url = state_clone.db.get_setting("validation_url")
            .ok().flatten()
            .unwrap_or_else(|| state_clone.config.validation.url.clone());
        let timeout = std::time::Duration::from_secs(
            state_clone.db.get_setting("validation_timeout_secs")
                .ok().flatten().and_then(|v| v.parse().ok())
                .unwrap_or(state_clone.config.validation.timeout_secs)
        );

        // Get or create binding
        let (local_port, temp_binding) = match state_clone.pool.get(&proxy_id) {
            Some(p) if p.local_port.is_some() => (p.local_port.unwrap(), false),
            Some(p) => {
                let mut mgr = state_clone.singbox.lock().await;
                match mgr.create_binding(&proxy_id, &p.singbox_outbound).await {
                    Ok(port) => (port, true),
                    Err(e) => {
                        tracing::error!("Failed to create temp binding for {}: {e}", proxy_id);
                        return;
                    }
                }
            }
            None => return,
        };

        // Run validation
        let proxy_addr = format!("http://127.0.0.1:{local_port}");
        let result = validate_through_proxy(&proxy_addr, &validation_url, timeout).await;

        match result {
            Ok(()) => {
                state_clone.pool.set_status(&proxy_id, crate::pool::manager::ProxyStatus::Valid);
                state_clone.db.update_proxy_validation(&proxy_id, true, None).ok();
                tracing::info!("Single validation OK: {}", proxy_id);
            }
            Err(e) => {
                state_clone.pool.set_status(&proxy_id, crate::pool::manager::ProxyStatus::Invalid);
                state_clone.db.update_proxy_validation(&proxy_id, false, Some(&e)).ok();
                tracing::info!("Single validation FAILED: {} — {e}", proxy_id);
            }
        }

        // Cleanup temp binding
        if temp_binding {
            let mut mgr = state_clone.singbox.lock().await;
            mgr.remove_binding(&proxy_id, local_port).await.ok();
        }
    });

    Ok(Json(json!({ "message": "Validation started for proxy" })))
}

/// Reusable single-proxy HTTP validation
async fn validate_through_proxy(
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
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| format!("Client build error: {e}"))?;

    let resp = client.get(target_url).send().await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() || resp.status().is_redirection() {
        Ok(())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}

pub async fn quality_check_single_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    if proxy.is_disabled {
        return Err(AppError::BadRequest("Cannot quality-check a disabled proxy".into()));
    }
    if proxy.status != crate::pool::manager::ProxyStatus::Valid {
        return Err(AppError::BadRequest("Proxy must be valid before quality check".into()));
    }

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        // Ensure binding exists
        let temp_binding = if state_clone.pool.get(&proxy_id).map(|p| p.local_port.is_none()).unwrap_or(true) {
            if let Some(p) = state_clone.pool.get(&proxy_id) {
                let mut mgr = state_clone.singbox.lock().await;
                match mgr.create_binding(&proxy_id, &p.singbox_outbound).await {
                    Ok(port) => {
                        state_clone.pool.set_local_port(&proxy_id, port);
                        Some(port)
                    }
                    Err(e) => {
                        tracing::error!("Temp binding for quality check failed: {e}");
                        return;
                    }
                }
            } else { return; }
        } else { None };

        match crate::quality::checker::check_single_proxy(&state_clone, &proxy_id).await {
            Ok(()) => tracing::info!("Single quality check OK: {proxy_id}"),
            Err(e) => tracing::warn!("Single quality check failed for {proxy_id}: {e}"),
        }

        // Cleanup temp binding
        if let Some(port) = temp_binding {
            state_clone.pool.clear_local_port(&proxy_id);
            let mut mgr = state_clone.singbox.lock().await;
            mgr.remove_binding(&proxy_id, port).await.ok();
        }
    });

    Ok(Json(json!({ "message": "Quality check started for proxy" })))
}
