use crate::error::AppError;
use crate::AppState;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher};
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

pub async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let users = state.db.get_all_users()?;
    let total = users.len();
    Ok(Json(json!({
        "users": users,
        "total": total,
    })))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.db.delete_user(&id)?;
    Ok(Json(json!({ "message": "User deleted" })))
}

pub async fn ban_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.db.set_user_banned(&id, true)?;
    Ok(Json(json!({ "message": "User banned" })))
}

pub async fn unban_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.db.set_user_banned(&id, false)?;
    Ok(Json(json!({ "message": "User unbanned" })))
}

// --- Password User Management ---

#[derive(Debug, serde::Deserialize)]
pub struct CreatePasswordUserRequest {
    pub username: String,
    pub password: String,
}

pub async fn create_password_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePasswordUserRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if req.username.is_empty() || req.password.is_empty() {
        return Err(AppError::Internal("Username and password are required".into()));
    }

    // Check if username already exists
    if state.db.get_user_by_username(&req.username)?.is_some() {
        return Err(AppError::Internal("Username already exists".into()));
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Hash error: {e}")))?
        .to_string();

    let trust_level = state.db.get_setting("linuxdo_min_trust_level")
        .ok().flatten().and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let user = state.db.create_password_user(&req.username, &hash, trust_level, "user")?;

    Ok(Json(json!({
        "message": "User created",
        "user": {
            "id": user.id,
            "username": user.username,
            "api_key": user.api_key,
            "auth_source": user.auth_source,
        }
    })))
}

#[derive(Debug, serde::Deserialize)]
pub struct ResetPasswordRequest {
    pub password: String,
}

pub async fn reset_user_password(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if req.password.is_empty() {
        return Err(AppError::Internal("Password is required".into()));
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Hash error: {e}")))?
        .to_string();

    state.db.update_user_password(&id, &hash)?;

    Ok(Json(json!({ "message": "Password updated" })))
}
