use crate::error::AppError;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

#[derive(serde::Deserialize)]
pub struct BatchRequest {
    action: String,
    ids: Vec<String>,
}

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
    // Check if validation is running — don't block, return friendly error
    if state.validation_lock.try_lock().is_err() {
        return Err(AppError::Conflict("验证进行中，请稍后操作".into()));
    }

    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    let new_disabled = !proxy.is_disabled;
    state.pool.set_disabled(&id, new_disabled);
    state.db.set_proxy_disabled(&id, new_disabled)?;

    if new_disabled {
        // Disabling: release sing-box binding but KEEP DB local_port (port memory)
        if let Some(port) = proxy.local_port {
            let mut mgr = state.singbox.lock().await;
            mgr.remove_binding(&id, port).await.ok();
            state.pool.clear_local_port(&id);
            // DON'T clear DB local_port — that's port memory
        }
    } else {
        // Enabling: sync bindings to assign port (will try to restore remembered port)
        let state2 = state.clone();
        tokio::spawn(async move {
            crate::api::subscription::sync_proxy_bindings(&state2).await;
        });
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
    state
        .pool
        .get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        run_single_validation(state_clone, proxy_id).await;
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

fn remembered_proxy_port(db: &crate::db::Database, proxy_id: &str) -> Option<u16> {
    db.get_all_proxies()
        .ok()
        .and_then(|rows| {
            rows.into_iter()
                .find(|row| row.id == proxy_id)
                .and_then(|row| row.local_port.map(|port| port as u16))
        })
}

async fn ensure_proxy_binding(
    state: &Arc<AppState>,
    proxy: &crate::pool::manager::PoolProxy,
    sync_pool_port: bool,
) -> Result<(u16, bool), String> {
    if let Some(port) = proxy.local_port {
        return Ok((port, false));
    }

    let remembered_port = remembered_proxy_port(&state.db, &proxy.id);
    let mut mgr = state.singbox.lock().await;
    let local_port = match remembered_port {
        Some(port) => mgr
            .create_binding_on_port(&proxy.id, port, &proxy.singbox_outbound)
            .await?,
        None => mgr.create_binding(&proxy.id, &proxy.singbox_outbound).await?,
    };

    if sync_pool_port {
        state.pool.set_local_port(&proxy.id, local_port);
    }

    Ok((local_port, true))
}

async fn cleanup_temp_binding(
    state: &Arc<AppState>,
    proxy_id: &str,
    local_port: u16,
    clear_pool_port: bool,
) {
    if clear_pool_port {
        state.pool.clear_local_port(proxy_id);
    }

    let mut mgr = state.singbox.lock().await;
    mgr.remove_binding(proxy_id, local_port).await.ok();
}

async fn run_single_validation(state: Arc<AppState>, proxy_id: String) {
    let validation_url = state
        .db
        .get_setting("validation_url")
        .ok()
        .flatten()
        .unwrap_or_else(|| state.config.validation.url.clone());
    let timeout = std::time::Duration::from_secs(
        state
            .db
            .get_setting("validation_timeout_secs")
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(state.config.validation.timeout_secs),
    );

    let Some(proxy) = state.pool.get(&proxy_id) else {
        return;
    };

    let (local_port, temp_binding) = match ensure_proxy_binding(&state, &proxy, false).await {
        Ok(binding) => binding,
        Err(e) => {
            tracing::error!("Failed to create temp binding for {}: {e}", proxy_id);
            return;
        }
    };

    let proxy_addr = format!("http://127.0.0.1:{local_port}");
    let result = validate_through_proxy(&proxy_addr, &validation_url, timeout).await;

    match result {
        Ok(()) => {
            state
                .pool
                .set_status(&proxy_id, crate::pool::manager::ProxyStatus::Valid);
            state.db.update_proxy_validation(&proxy_id, true, None).ok();
            tracing::info!("Single validation OK: {}", proxy_id);
        }
        Err(e) => {
            state
                .pool
                .set_status(&proxy_id, crate::pool::manager::ProxyStatus::Invalid);
            state
                .db
                .update_proxy_validation(&proxy_id, false, Some(&e))
                .ok();
            tracing::info!("Single validation FAILED: {} — {e}", proxy_id);
        }
    }

    if temp_binding {
        cleanup_temp_binding(&state, &proxy_id, local_port, false).await;
    }
}

async fn run_single_quality_check(state: Arc<AppState>, proxy_id: String) {
    let Some(proxy) = state.pool.get(&proxy_id) else {
        return;
    };

    if proxy.status != crate::pool::manager::ProxyStatus::Valid {
        tracing::warn!("Skipping single quality check for non-valid proxy {}", proxy_id);
        return;
    }

    let (local_port, temp_binding) = match ensure_proxy_binding(&state, &proxy, true).await {
        Ok(binding) => binding,
        Err(e) => {
            tracing::error!("Temp binding for quality check failed: {e}");
            return;
        }
    };

    if temp_binding {
        state.pool.set_local_port(&proxy_id, local_port);
    }

    match crate::quality::checker::check_single_proxy(&state, &proxy_id).await {
        Ok(()) => tracing::info!("Single quality check OK: {proxy_id}"),
        Err(e) => tracing::warn!("Single quality check failed for {proxy_id}: {e}"),
    }

    if temp_binding {
        cleanup_temp_binding(&state, &proxy_id, local_port, true).await;
    }
}

pub async fn quality_check_single_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    if proxy.status != crate::pool::manager::ProxyStatus::Valid {
        return Err(AppError::BadRequest("Proxy must be valid before quality check".into()));
    }

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        run_single_quality_check(state_clone, proxy_id).await;
    });

    Ok(Json(json!({ "message": "Quality check started for proxy" })))
}

/// Batch enable: enable all proxies that are currently Valid + disabled.
pub async fn batch_enable_valid(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.validation_lock.try_lock().is_err() {
        return Err(AppError::Conflict("验证进行中，请稍后操作".into()));
    }

    let all_targets: Vec<String> = state.pool.get_all()
        .iter()
        .filter(|p| p.is_disabled && p.status == crate::pool::manager::ProxyStatus::Valid)
        .map(|p| p.id.clone())
        .collect();

    let count = all_targets.len();
    for id in &all_targets {
        state.pool.set_disabled(id, false);
        state.db.set_proxy_disabled(id, false).ok();
    }

    if count > 0 {
        // Sync bindings to assign ports
        let state2 = state.clone();
        tokio::spawn(async move {
            crate::api::subscription::sync_proxy_bindings(&state2).await;
        });
    }

    tracing::info!("Batch enable-valid: enabled {} proxies", count);
    Ok(Json(json!({
        "message": format!("已启用 {} 个有效代理", count),
        "enabled_count": count,
    })))
}

/// Batch disable: disable all proxies that are currently Invalid + enabled.
pub async fn batch_disable_invalid(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.validation_lock.try_lock().is_err() {
        return Err(AppError::Conflict("验证进行中，请稍后操作".into()));
    }

    let targets: Vec<_> = state.pool.get_all()
        .into_iter()
        .filter(|p| !p.is_disabled && p.status == crate::pool::manager::ProxyStatus::Invalid)
        .collect();

    let count = targets.len();
    for p in &targets {
        state.pool.set_disabled(&p.id, true);
        state.db.set_proxy_disabled(&p.id, true).ok();

        // Release sing-box binding but keep DB local_port (port memory)
        if let Some(port) = p.local_port {
            let mut mgr = state.singbox.lock().await;
            mgr.remove_binding(&p.id, port).await.ok();
            state.pool.clear_local_port(&p.id);
        }
    }

    tracing::info!("Batch disable-invalid: disabled {} proxies", count);
    Ok(Json(json!({
        "message": format!("已禁用 {} 个无效代理", count),
        "disabled_count": count,
    })))
}

/// Batch validate: validate only disabled proxies in background.
pub async fn batch_validate_disabled(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::pool::validator::validate_disabled_only(state_clone).await {
            tracing::error!("Batch validate-disabled failed: {e}");
        }
    });

    Ok(Json(json!({
        "message": "已开始验证禁用代理",
    })))
}

pub async fn batch_validate_invalid(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::pool::validator::validate_invalid_only(state_clone).await {
            tracing::error!("Batch validate-invalid failed: {e}");
        }
    });

    Ok(Json(json!({
        "message": "已开始验证无效代理",
    })))
}

pub async fn batch_proxy_action(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.validation_lock.try_lock().is_err()
        && matches!(req.action.as_str(), "enable" | "disable")
    {
        return Err(AppError::Conflict("验证进行中，请稍后操作".into()));
    }

    let proxies: Vec<_> = req
        .ids
        .iter()
        .filter_map(|id| state.pool.get(id))
        .collect();
    let total = proxies.len();

    match req.action.as_str() {
        "enable" => {
            let targets: Vec<_> = proxies.into_iter().filter(|p| p.is_disabled).collect();
            let processed = targets.len();
            for proxy in &targets {
                state.pool.set_disabled(&proxy.id, false);
                state.db.set_proxy_disabled(&proxy.id, false).ok();
            }
            if processed > 0 {
                let state2 = state.clone();
                tokio::spawn(async move {
                    crate::api::subscription::sync_proxy_bindings(&state2).await;
                });
            }
            Ok(Json(json!({
                "action": "enable",
                "total": total,
                "processed": processed,
                "skipped": total - processed,
                "message": format!("已启用 {} 个，跳过 {} 个(已启用)", processed, total - processed),
            })))
        }
        "disable" => {
            let targets: Vec<_> = proxies.into_iter().filter(|p| !p.is_disabled).collect();
            let processed = targets.len();
            for proxy in &targets {
                state.pool.set_disabled(&proxy.id, true);
                state.db.set_proxy_disabled(&proxy.id, true).ok();
                if let Some(port) = proxy.local_port {
                    let mut mgr = state.singbox.lock().await;
                    mgr.remove_binding(&proxy.id, port).await.ok();
                    state.pool.clear_local_port(&proxy.id);
                }
            }
            Ok(Json(json!({
                "action": "disable",
                "total": total,
                "processed": processed,
                "skipped": total - processed,
                "message": format!("已禁用 {} 个，跳过 {} 个(已禁用)", processed, total - processed),
            })))
        }
        "validate" => {
            let ids = req.ids.clone();
            let state2 = state.clone();
            tokio::spawn(async move {
                for id in ids {
                    run_single_validation(state2.clone(), id).await;
                }
            });
            Ok(Json(json!({
                "action": "validate",
                "total": total,
                "processed": total,
                "skipped": 0,
                "message": format!("已启动 {} 个代理的连通测试", total),
            })))
        }
        "quality" => {
            let targets: Vec<_> = proxies
                .iter()
                .filter(|p| p.status == crate::pool::manager::ProxyStatus::Valid)
                .map(|p| p.id.clone())
                .collect();
            let processed = targets.len();
            let skipped = total - processed;
            let state2 = state.clone();
            tokio::spawn(async move {
                for id in targets {
                    run_single_quality_check(state2.clone(), id).await;
                }
            });
            Ok(Json(json!({
                "action": "quality",
                "total": total,
                "processed": processed,
                "skipped": skipped,
                "message": format!("已启动 {} 个代理的质检，跳过 {} 个(非有效)", processed, skipped),
            })))
        }
        "delete" => {
            for proxy in &proxies {
                if let Some(port) = proxy.local_port {
                    let mut mgr = state.singbox.lock().await;
                    mgr.remove_binding(&proxy.id, port).await.ok();
                }
                state.pool.remove(&proxy.id);
                state.db.delete_proxy(&proxy.id).ok();
            }
            Ok(Json(json!({
                "action": "delete",
                "total": total,
                "processed": total,
                "skipped": 0,
                "message": format!("已删除 {} 个代理", total),
            })))
        }
        _ => Err(AppError::BadRequest("Unknown batch action".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::remembered_proxy_port;
    use crate::db::{Database, ProxyRow};
    use std::path::Path;

    fn sample_proxy_row(id: &str, local_port: Option<i32>) -> ProxyRow {
        ProxyRow {
            id: id.to_string(),
            subscription_id: "sub-1".to_string(),
            name: format!("Proxy {id}"),
            proxy_type: "vmess".to_string(),
            server: "example.com".to_string(),
            port: 443,
            config_json: "{}".to_string(),
            is_valid: false,
            local_port,
            error_count: 0,
            last_error: None,
            last_validated: None,
            created_at: "2026-03-28T00:00:00Z".to_string(),
            updated_at: "2026-03-28T00:00:00Z".to_string(),
            is_disabled: false,
            disabled_at: None,
        }
    }

    #[test]
    fn remembered_proxy_port_reads_saved_port_from_db() {
        let db = Database::new(Path::new(":memory:")).expect("in-memory db should initialize");
        db.insert_proxy(&sample_proxy_row("with-port", Some(61001)))
            .expect("proxy row should insert");
        db.insert_proxy(&sample_proxy_row("without-port", None))
            .expect("proxy row should insert");

        assert_eq!(remembered_proxy_port(&db, "with-port"), Some(61001));
        assert_eq!(remembered_proxy_port(&db, "without-port"), None);
        assert_eq!(remembered_proxy_port(&db, "missing"), None);
    }
}
