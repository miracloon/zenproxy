use crate::config::write_settings_to_config;
use crate::error::AppError;
use crate::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stats = state.db.get_stats()?;
    Ok(Json(stats))
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let settings = state.db.get_all_settings()?;
    Ok(Json(json!(settings)))
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // 1. Write to DB
    state.db.set_all_settings(&req)?;

    // 2. Write back to config.toml
    if let Err(e) = write_settings_to_config(&req, &state.config_path) {
        tracing::error!("Failed to write settings to config file: {e}");
        return Err(AppError::Internal(format!("Settings saved to DB but config file write failed: {e}")));
    }

    tracing::info!("Settings updated via admin UI ({} keys)", req.len());
    Ok(Json(json!({ "message": "Settings saved" })))
}
