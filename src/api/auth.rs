use crate::db::User;
use crate::error::AppError;
use crate::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::PasswordHasher;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

const AUTHORIZE_URL: &str = "https://connect.linux.do/oauth2/authorize";
const TOKEN_URL: &str = "https://connect.linux.do/oauth2/token";
const USERINFO_URL: &str = "https://connect.linux.do/api/user";
pub const COOKIE_NAME: &str = "zenproxy_session";

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct LinuxDoUser {
    id: i64,
    username: String,
    name: Option<String>,
    avatar_template: Option<String>,
    active: Option<bool>,
    trust_level: Option<i32>,
    silenced: Option<bool>,
}

pub async fn login(State(state): State<Arc<AppState>>) -> Response {
    // Check if OAuth is enabled
    let enabled = state.db.get_setting("linuxdo_oauth_enabled")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(true);
    if !enabled {
        return (axum::http::StatusCode::FORBIDDEN, "OAuth login is disabled").into_response();
    }

    let client_id = state.db.get_setting("linuxdo_client_id")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.client_id.clone());
    let redirect_uri = state.db.get_setting("linuxdo_redirect_uri")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.redirect_uri.clone());
    let url = format!(
        "{AUTHORIZE_URL}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code"
    );
    Redirect::temporary(&url).into_response()
}

pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    // Check if OAuth is enabled
    let enabled = state.db.get_setting("linuxdo_oauth_enabled")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(true);
    if !enabled {
        return Err(AppError::Forbidden("OAuth login is disabled".into()));
    }

    let client = reqwest::Client::new();

    let client_id = state.db.get_setting("linuxdo_client_id")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.client_id.clone());
    let client_secret = state.db.get_setting("linuxdo_client_secret")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.client_secret.clone());
    let redirect_uri = state.db.get_setting("linuxdo_redirect_uri")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.redirect_uri.clone());

    // Exchange code for token
    let token_resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &query.code),
            ("client_id", &client_id),
            ("client_secret", &client_secret),
            ("redirect_uri", &redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Token exchange failed: {e}")))?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!("Token exchange error: {body}")));
    }

    let token: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Token parse error: {e}")))?;

    // Fetch user info
    let user_resp = client
        .get(USERINFO_URL)
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("User info fetch failed: {e}")))?;

    if !user_resp.status().is_success() {
        let body = user_resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!("User info error: {body}")));
    }

    let ldo_user: LinuxDoUser = user_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("User info parse error: {e}")))?;

    let trust_level = ldo_user.trust_level.unwrap_or(0);
    let min_trust = state.db.get_setting("linuxdo_min_trust_level")
        .ok().flatten().and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(state.config.oauth.linuxdo.min_trust_level);

    if trust_level < min_trust {
        // Return an HTML error page instead of JSON for OAuth callback
        let html = format!(
            r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Access Denied</title>
            <style>body{{font-family:system-ui;background:#0f1117;color:#e2e8f0;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}}
            .box{{background:#1a1d27;border:1px solid #2a2d3a;border-radius:16px;padding:40px;text-align:center;max-width:400px}}
            h2{{color:#ef4444;margin-bottom:12px}}a{{color:#6c63ff}}</style></head>
            <body><div class="box"><h2>Access Denied</h2>
            <p>Your trust level ({trust_level}) is below the minimum required ({min_trust}).</p>
            <p style="margin-top:16px"><a href="/">Back</a></p></div></body></html>"#
        );
        return Ok(axum::response::Html(html).into_response());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let user_id = ldo_user.id.to_string();

    // Check if user exists to preserve api_key
    let api_key = match state.db.get_user_by_id(&user_id)? {
        Some(existing) => {
            if existing.is_banned {
                let html = r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Banned</title>
                <style>body{font-family:system-ui;background:#0f1117;color:#e2e8f0;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}
                .box{background:#1a1d27;border:1px solid #2a2d3a;border-radius:16px;padding:40px;text-align:center;max-width:400px}
                h2{color:#ef4444;margin-bottom:12px}a{color:#6c63ff}</style></head>
                <body><div class="box"><h2>Account Banned</h2>
                <p>Your account has been banned by the administrator.</p>
                <p style="margin-top:16px"><a href="/">Back</a></p></div></body></html>"#;
                return Ok(axum::response::Html(html).into_response());
            }
            existing.api_key
        }
        None => uuid::Uuid::new_v4().to_string(),
    };

    let user = User {
        id: user_id.clone(),
        username: ldo_user.username,
        name: ldo_user.name,
        avatar_template: ldo_user.avatar_template,
        active: ldo_user.active.unwrap_or(true),
        trust_level,
        silenced: ldo_user.silenced.unwrap_or(false),
        is_banned: false,
        api_key,
        created_at: now.clone(),
        updated_at: now,
        password_hash: None,
        auth_source: "oauth".to_string(),
        role: "user".to_string(),
    };

    state.db.upsert_user(&user)?;

    // Create session
    let session = state.db.create_session(&user_id)?;

    // Set cookie and redirect
    let redirect_uri_for_secure = state.db.get_setting("linuxdo_redirect_uri")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.redirect_uri.clone());
    let secure = if redirect_uri_for_secure.starts_with("https") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800{secure}",
        session.id
    );
    let mut response = Redirect::temporary("/").into_response();
    response
        .headers_mut()
        .insert("Set-Cookie", HeaderValue::from_str(&cookie).unwrap());
    Ok(response)
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = extract_session_user(&state, &headers).await?;
    Ok(Json(json!({
        "id": user.id,
        "username": user.username,
        "name": user.name,
        "avatar_template": user.avatar_template,
        "trust_level": user.trust_level,
        "api_key": user.api_key,
        "created_at": user.created_at,
        "role": user.role,
    })))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(session_id) = extract_session_id(&headers) {
        state.db.delete_session(&session_id)?;
    }
    let redirect_uri_for_secure = state.db.get_setting("linuxdo_redirect_uri")
        .ok().flatten()
        .unwrap_or_else(|| state.config.oauth.linuxdo.redirect_uri.clone());
    let secure = if redirect_uri_for_secure.starts_with("https") {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}");
    let mut response = Json(json!({ "message": "Logged out" })).into_response();
    response
        .headers_mut()
        .insert("Set-Cookie", HeaderValue::from_str(&cookie).unwrap());
    Ok(response)
}

pub async fn regenerate_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = extract_session_user(&state, &headers).await?;
    let new_key = state.db.regenerate_api_key(&user.id)?;
    Ok(Json(json!({ "api_key": new_key })))
}

// --- Helper functions ---

pub fn extract_session_id(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&format!("{COOKIE_NAME}=")) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

pub async fn extract_session_user(state: &AppState, headers: &HeaderMap) -> Result<User, AppError> {
    let session_id = extract_session_id(headers)
        .ok_or_else(|| AppError::Unauthorized("No session cookie".into()))?;

    let session = state
        .db
        .get_session(&session_id)?
        .ok_or_else(|| AppError::Unauthorized("Invalid session".into()))?;

    // Check expiry
    let expires = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
        .map_err(|_| AppError::Unauthorized("Invalid session expiry".into()))?;
    if chrono::Utc::now() > expires {
        state.db.delete_session(&session_id)?;
        return Err(AppError::Unauthorized("Session expired".into()));
    }

    let user = state
        .db
        .get_user_by_id(&session.user_id)?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    if user.is_banned {
        state.db.delete_user_sessions(&user.id)?;
        return Err(AppError::Unauthorized("Account banned".into()));
    }

    Ok(user)
}

pub async fn extract_api_key_user(
    state: &AppState,
    headers: &HeaderMap,
    query_api_key: Option<&str>,
) -> Result<User, AppError> {
    // Try Authorization: Bearer <api_key> header first
    let api_key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| query_api_key.map(|s| s.to_string()));

    if let Some(key) = api_key {
        let user = state
            .db
            .get_user_by_api_key(&key)?
            .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

        if user.is_banned {
            return Err(AppError::Unauthorized("Account banned".into()));
        }

        return Ok(user);
    }

    Err(AppError::Unauthorized("No API key provided".into()))
}

/// Cache TTL for auth lookups — avoids hitting DB mutex on every relay request.
const AUTH_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// Try API key first, then session cookie. Uses in-memory cache.
pub async fn authenticate_request(
    state: &AppState,
    headers: &HeaderMap,
    query_api_key: Option<&str>,
) -> Result<User, AppError> {
    // Try API key (from header or query)
    let api_key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| query_api_key.map(|s| s.to_string()));

    if let Some(ref key) = api_key {
        let cache_key = format!("ak:{key}");
        if let Some(user) = get_cached_user(state, &cache_key) {
            return Ok(user);
        }
        if let Ok(user) = extract_api_key_user(state, headers, query_api_key).await {
            cache_user(state, &cache_key, &user);
            return Ok(user);
        }
    }

    // Try session cookie
    if let Some(session_id) = extract_session_id(headers) {
        let cache_key = format!("ss:{session_id}");
        if let Some(user) = get_cached_user(state, &cache_key) {
            return Ok(user);
        }
        if let Ok(user) = extract_session_user(state, headers).await {
            cache_user(state, &cache_key, &user);
            return Ok(user);
        }
    }

    Err(AppError::Unauthorized(
        "Authentication required. Provide an API key or login via OAuth.".into(),
    ))
}

fn get_cached_user(state: &AppState, cache_key: &str) -> Option<User> {
    let entry = state.auth_cache.get(cache_key)?;
    let (user, expires) = entry.value();
    if tokio::time::Instant::now() < *expires {
        Some(user.clone())
    } else {
        // Don't remove here — avoids TOCTOU race where a concurrent insert
        // could be deleted. Let the periodic cleanup task handle expired entries.
        None
    }
}

fn cache_user(state: &AppState, cache_key: &str, user: &User) {
    let expires = tokio::time::Instant::now() + AUTH_CACHE_TTL;
    state.auth_cache.insert(cache_key.to_string(), (user.clone(), expires));
}

// --- Password Login ---

#[derive(Debug, Deserialize)]
pub struct PasswordLoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login_password(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PasswordLoginRequest>,
) -> Result<Response, AppError> {
    let user = state.db.get_user_by_username(&req.username)?
        .ok_or_else(|| AppError::Unauthorized("Invalid username or password".into()))?;

    if user.is_banned {
        return Err(AppError::Unauthorized("Account banned".into()));
    }

    let hash_str = user.password_hash.as_deref()
        .ok_or_else(|| AppError::Unauthorized("Invalid username or password".into()))?;

    let parsed_hash = PasswordHash::new(hash_str)
        .map_err(|_| AppError::Internal("Password hash error".into()))?;

    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::Unauthorized("Invalid username or password".into()))?;

    // Create session (same as OAuth callback flow)
    let session = state.db.create_session(&user.id)?;

    let cookie = format!(
        "{COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800",
        session.id
    );
    let mut response = Json(json!({ "message": "Login successful" })).into_response();
    response.headers_mut()
        .insert("Set-Cookie", HeaderValue::from_str(&cookie).unwrap());
    Ok(response)
}

// --- Auth Options & Registration ---

/// Public endpoint — returns which login/register methods are available.
pub async fn auth_options(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let enable_oauth = state.db.get_setting("linuxdo_oauth_enabled")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(true);
    let allow_registration = state.db.get_setting("allow_registration")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(false);

    Json(json!({
        "linuxdo_oauth_enabled": enable_oauth,
        "allow_registration": allow_registration,
    }))
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PasswordLoginRequest>,
) -> Result<Response, AppError> {
    // Check if registration is allowed
    let allowed = state.db.get_setting("allow_registration")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    if !allowed {
        return Err(AppError::Forbidden("User registration is disabled".into()));
    }

    if req.username.is_empty() || req.password.is_empty() {
        return Err(AppError::BadRequest("Username and password are required".into()));
    }

    // Check if username exists
    if state.db.get_user_by_username(&req.username)?.is_some() {
        return Err(AppError::Conflict("Username already exists".into()));
    }

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Hash error: {e}")))?
        .to_string();

    let min_trust = state.db.get_setting("linuxdo_min_trust_level")
        .ok().flatten()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(1);

    let user = state.db.create_password_user(&req.username, &hash, min_trust, "user")?;

    // Auto-login: create session
    let session = state.db.create_session(&user.id)?;
    let cookie = format!(
        "{COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800",
        session.id
    );
    let mut response = Json(json!({ "message": "Registration successful" })).into_response();
    response.headers_mut()
        .insert("Set-Cookie", HeaderValue::from_str(&cookie).unwrap());
    Ok(response)
}
