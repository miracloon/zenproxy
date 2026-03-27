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
    current_user: super::CurrentUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target = state.db.get_user_by_id(&id)?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Cannot delete yourself
    if current_user.id == id {
        return Err(AppError::BadRequest("Cannot delete your own account".into()));
    }

    match (current_user.role.as_str(), target.role.as_str()) {
        ("super_admin", "super_admin") => {
            // Check: must keep at least 1 super_admin
            let count = state.db.count_users_by_role("super_admin")?;
            if count <= 1 {
                return Err(AppError::BadRequest("Cannot delete the last super_admin".into()));
            }
        }
        ("super_admin", _) => { /* OK */ }
        ("admin", "user") => { /* OK */ }
        ("admin", _) => {
            return Err(AppError::Forbidden("Admin can only delete user-level accounts".into()));
        }
        _ => return Err(AppError::Forbidden("Insufficient permissions".into())),
    }

    // Delete user sessions first, then user
    state.db.delete_user_sessions(&id).ok();
    state.db.delete_user(&id)?;
    state.auth_cache.retain(|_, (u, _)| u.id != id);

    tracing::info!("User {} deleted by {}", target.username, current_user.username);
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

// --- Role Management ---

#[derive(Debug, serde::Deserialize)]
pub struct ChangeRoleRequest {
    pub role: String,
}

pub async fn change_user_role(
    current_user: super::CurrentUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ChangeRoleRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Validate role value
    if !["user", "admin", "super_admin"].contains(&req.role.as_str()) {
        return Err(AppError::BadRequest("Invalid role. Must be 'user', 'admin', or 'super_admin'".into()));
    }

    let target = state.db.get_user_by_id(&id)?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Permission checks
    match current_user.role.as_str() {
        "super_admin" => {
            // super_admin can change anyone to any role
        }
        "admin" => {
            // admin can only change user↔admin
            if target.role == "super_admin" {
                return Err(AppError::Forbidden("Cannot modify super_admin's role".into()));
            }
            if req.role == "super_admin" {
                return Err(AppError::Forbidden("Only super_admin can promote to super_admin".into()));
            }
        }
        _ => return Err(AppError::Forbidden("Insufficient permissions".into())),
    }

    state.db.update_user_role(&id, &req.role)?;
    // Invalidate auth cache for this user
    state.auth_cache.retain(|_, (u, _)| u.id != id);

    tracing::info!("User {} role changed to {} by {}", target.username, req.role, current_user.username);
    Ok(Json(json!({ "message": format!("Role updated to {}", req.role) })))
}
