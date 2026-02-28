use crate::db::{ProxyRow, Subscription};
use crate::error::AppError;
use crate::parser;
use crate::pool::manager::{PoolProxy, ProxyStatus};
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AddSubscriptionRequest {
    pub name: String,
    #[serde(rename = "type", default = "default_sub_type")]
    pub sub_type: String,
    pub url: Option<String>,
    pub content: Option<String>,
}

fn default_sub_type() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Copy)]
pub enum SyncMode {
    Normal,
    Validation,
    QualityCheck,
}

pub async fn list_subscriptions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let subs = state.db.get_subscriptions()?;
    Ok(Json(json!({ "subscriptions": subs })))
}

pub async fn add_subscription(
    State(state): State<Arc<AppState>>,
    body: axum::body::Body,
) -> Result<Json<serde_json::Value>, AppError> {
    // Try to parse as JSON first
    let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024)
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read body: {e}")))?;

    let req: AddSubscriptionRequest = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {e}")))?;

    // Fetch content from URL or use provided content
    let content = if let Some(ref url) = req.url {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch subscription: {e}")))?;
        resp.text()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read response: {e}")))?
    } else if let Some(ref content) = req.content {
        content.clone()
    } else {
        return Err(AppError::BadRequest(
            "Either 'url' or 'content' must be provided".into(),
        ));
    };

    // Parse the content
    let parsed = parser::parse_subscription(&content, &req.sub_type);
    if parsed.is_empty() {
        return Err(AppError::BadRequest(
            "No proxies found in subscription content".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let sub_id = uuid::Uuid::new_v4().to_string();

    let subscription = Subscription {
        id: sub_id.clone(),
        name: req.name.clone(),
        sub_type: req.sub_type.clone(),
        url: req.url.clone(),
        content: Some(content),
        proxy_count: parsed.len() as i32,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    state.db.insert_subscription(&subscription)?;

    // Insert proxies
    let mut added = 0;
    for pc in &parsed {
        let proxy_id = uuid::Uuid::new_v4().to_string();
        let proxy_row = ProxyRow {
            id: proxy_id.clone(),
            subscription_id: sub_id.clone(),
            name: pc.name.clone(),
            proxy_type: pc.proxy_type.to_string(),
            server: pc.server.clone(),
            port: pc.port as i32,
            config_json: serde_json::to_string(&pc.singbox_outbound).unwrap_or_default(),
            is_valid: false,
            local_port: None,
            error_count: 0,
            last_error: None,
            last_validated: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        state.db.insert_proxy(&proxy_row)?;

        let pool_proxy = PoolProxy {
            id: proxy_id,
            subscription_id: sub_id.clone(),
            name: pc.name.clone(),
            proxy_type: pc.proxy_type.to_string(),
            server: pc.server.clone(),
            port: pc.port,
            singbox_outbound: pc.singbox_outbound.clone(),
            status: ProxyStatus::Untested,
            local_port: None,
            error_count: 0,
            quality: None,
        };
        state.pool.add(pool_proxy);
        added += 1;
    }

    tracing::info!("Added subscription '{}' with {added} proxies", req.name);

    // Assign ports then validate in background (must be sequential, not two separate spawns)
    let state2 = state.clone();
    tokio::spawn(async move {
        tracing::info!("Running initial validation for new proxies...");
        if let Err(e) = crate::pool::validator::validate_all(state2).await {
            tracing::error!("Initial validation failed: {e}");
        }
    });

    Ok(Json(json!({
        "subscription": subscription,
        "proxies_added": added,
    })))
}

pub async fn delete_subscription(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.pool.remove_by_subscription(&id);
    state.db.delete_subscription(&id)?;

    // Sync bindings in background
    let state2 = state.clone();
    tokio::spawn(async move {
        sync_proxy_bindings(&state2, SyncMode::Normal).await;
    });

    Ok(Json(json!({ "message": "Subscription deleted" })))
}

pub async fn refresh_subscription(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sub = state
        .db
        .get_subscription(&id)?
        .ok_or_else(|| AppError::NotFound("Subscription not found".into()))?;

    let content = if let Some(ref url) = sub.url {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch: {e}")))?;
        resp.text()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read: {e}")))?
    } else if let Some(ref content) = sub.content {
        content.clone()
    } else {
        return Err(AppError::BadRequest("No URL or content to refresh".into()));
    };

    let parsed = parser::parse_subscription(&content, &sub.sub_type);

    // Delete old proxies for this subscription
    state.pool.remove_by_subscription(&id);
    state.db.delete_proxies_by_subscription(&id)?;

    // Insert new proxies
    let now = chrono::Utc::now().to_rfc3339();
    let mut added = 0;
    for pc in &parsed {
        let proxy_id = uuid::Uuid::new_v4().to_string();
        let proxy_row = ProxyRow {
            id: proxy_id.clone(),
            subscription_id: id.clone(),
            name: pc.name.clone(),
            proxy_type: pc.proxy_type.to_string(),
            server: pc.server.clone(),
            port: pc.port as i32,
            config_json: serde_json::to_string(&pc.singbox_outbound).unwrap_or_default(),
            is_valid: false,
            local_port: None,
            error_count: 0,
            last_error: None,
            last_validated: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        state.db.insert_proxy(&proxy_row)?;

        let pool_proxy = PoolProxy {
            id: proxy_id,
            subscription_id: id.clone(),
            name: pc.name.clone(),
            proxy_type: pc.proxy_type.to_string(),
            server: pc.server.clone(),
            port: pc.port,
            singbox_outbound: pc.singbox_outbound.clone(),
            status: ProxyStatus::Untested,
            local_port: None,
            error_count: 0,
            quality: None,
        };
        state.pool.add(pool_proxy);
        added += 1;
    }

    state
        .db
        .update_subscription_proxy_count(&id, added as i32)?;

    // Validate in background
    let state2 = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::pool::validator::validate_all(state2).await {
            tracing::error!("Validation after refresh failed: {e}");
        }
    });

    Ok(Json(json!({
        "message": "Subscription refreshed",
        "proxies_added": added,
    })))
}

/// Sync proxy bindings dynamically without restarting sing-box.
pub async fn sync_proxy_bindings(state: &Arc<AppState>, mode: SyncMode) {
    let mut all_proxies = state.pool.get_all();
    if all_proxies.is_empty() {
        // Remove all existing bindings
        let current_ports: Vec<(String, u16)> = all_proxies
            .iter()
            .filter_map(|p| p.local_port.map(|port| (p.id.clone(), port)))
            .collect();
        if !current_ports.is_empty() {
            let mut mgr = state.singbox.lock().await;
            for (id, port) in &current_ports {
                let _ = mgr.remove_binding(id, *port).await;
            }
        }
        return;
    }

    let max = state.config.singbox.max_proxies;

    match mode {
        SyncMode::Validation => {
            // Untested first (they need testing), then Valid, then Invalid
            all_proxies.sort_by(|a, b| {
                let weight = |s: ProxyStatus| -> u8 {
                    match s {
                        ProxyStatus::Untested => 0,
                        ProxyStatus::Valid => 1,
                        ProxyStatus::Invalid => 2,
                    }
                };
                weight(a.status)
                    .cmp(&weight(b.status))
                    .then_with(|| a.error_count.cmp(&b.error_count))
            });
        }
        SyncMode::QualityCheck => {
            // Valid-without-quality first, then Valid-with-quality, then rest
            all_proxies.sort_by(|a, b| {
                let weight = |p: &PoolProxy| -> u8 {
                    match p.status {
                        ProxyStatus::Valid if p.quality.is_none() => 0,
                        ProxyStatus::Valid => 1,
                        ProxyStatus::Untested => 2,
                        ProxyStatus::Invalid => 3,
                    }
                };
                weight(a)
                    .cmp(&weight(b))
                    .then_with(|| a.error_count.cmp(&b.error_count))
            });
        }
        SyncMode::Normal => {
            // Valid first (serve traffic), then Untested, then Invalid
            all_proxies.sort_by(|a, b| {
                a.status
                    .sort_weight()
                    .cmp(&b.status.sort_weight())
                    .then_with(|| a.error_count.cmp(&b.error_count))
            });
        }
    }

    // Split into selected (get ports) and rest (ports cleared)
    let cap = max.min(all_proxies.len());
    let selected: Vec<_> = all_proxies.drain(..cap).collect();
    let rest = all_proxies;

    // Clear ports for proxies that won't be loaded
    for p in &rest {
        if p.local_port.is_some() {
            state.pool.clear_local_port(&p.id);
            state.db.update_proxy_local_port_null(&p.id).ok();
        }
    }

    // Compute desired set and current bindings
    let desired: Vec<(String, serde_json::Value)> = selected
        .iter()
        .map(|p| (p.id.clone(), p.singbox_outbound.clone()))
        .collect();

    // Current ports: all proxies that currently have a local_port assigned
    // (includes both selected and rest — rest ports already cleared above in pool but
    //  we need the old state for sync_bindings to know what to remove)
    let current_ports: Vec<(String, u16)> = selected
        .iter()
        .chain(rest.iter())
        .filter_map(|p| p.local_port.map(|port| (p.id.clone(), port)))
        .collect();

    let mode_str = match mode {
        SyncMode::Normal => "normal",
        SyncMode::Validation => "validation",
        SyncMode::QualityCheck => "quality-check",
    };
    tracing::info!(
        "Syncing bindings for {}/{} proxies (max_proxies={}, mode={})",
        selected.len(),
        selected.len() + rest.len(),
        max,
        mode_str,
    );

    // Sync via SingboxManager
    let mut mgr = state.singbox.lock().await;
    let assignments = mgr.sync_bindings(&desired, &current_ports).await;
    drop(mgr);

    // Update pool and DB with new port assignments
    // First clear all selected ports, then set the ones we got
    for p in &selected {
        state.pool.clear_local_port(&p.id);
    }
    for (id, port) in &assignments {
        state.pool.set_local_port(id, *port);
        state.db.update_proxy_local_port(id, *port as i32).ok();
    }

    // Clear DB ports for proxies that didn't get assigned
    let assigned_ids: std::collections::HashSet<&str> =
        assignments.iter().map(|(id, _)| id.as_str()).collect();
    for p in &selected {
        if !assigned_ids.contains(p.id.as_str()) && p.local_port.is_some() {
            state.db.update_proxy_local_port_null(&p.id).ok();
        }
    }

    // Invalidate cached relay clients for ports no longer in use
    let active_ports: Vec<u16> = assignments.iter().map(|(_, port)| *port).collect();
    crate::api::relay::invalidate_relay_clients(state, &active_ports);
}
