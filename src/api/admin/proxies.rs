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
