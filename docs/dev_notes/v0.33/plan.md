# v0.33 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement unified config management (DB + config.toml + admin UI with save button) and user management enhancements (registration/OAuth toggles, login page restructuring).

**Architecture:** Settings stored in SQLite `settings` table, seeded from `config.toml` at startup. Admin UI Settings panel with "Save" button writes to DB + config.toml simultaneously. Login page dynamically renders based on `enable_oauth` and `allow_registration` settings.

**Tech Stack:** Rust (Axum, rusqlite, toml_edit, argon2), HTML/JS/CSS (inline in single-page templates)

**Design doc:** `docs/dev_notes/v0.33/design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Modify | Add `toml_edit` dependency |
| `src/db.rs` | Modify | Add `settings` table, CRUD methods |
| `src/config.rs` | Modify | Add new fields (`allow_registration`, `enable_oauth`), add `seed_settings`/`write_settings` functions |
| `src/main.rs` | Modify | Add `config_path` to AppState, call `seed_settings` at startup |
| `src/api/mod.rs` | Modify | Add new routes, update admin_auth to read from DB |
| `src/api/admin.rs` | Modify | Add `get_settings`/`update_settings` handlers |
| `src/api/auth.rs` | Modify | Add `auth_options`/`register` handlers, OAuth toggle check, runtime config reads |
| `src/api/subscription.rs` | Modify | Update `state.config.*` reads to use DB for runtime settings |
| `src/pool/validator.rs` | Modify | Read validation params from DB |
| `src/quality/checker.rs` | Modify | Read quality params from DB |
| `src/web/admin.html` | Modify | Add Settings panel with save button |
| `src/web/user.html` | Modify | Restructure login page (password first, dynamic OAuth/register) |
| `docker/server/config/config.toml` | Modify | Add `allow_registration`, `enable_oauth` fields |
| `docker/server/docker-compose.yml` | Modify | Remove `:ro` from config.toml mount |
| `docker/server/docker-compose-remote.yml` | Modify | Remove `:ro` from config.toml mount |

---

## Task 1: DB Settings Table

**Files:**
- Modify: `src/db.rs`

- [ ] **Step 1: Add settings table to migration**

In `src/db.rs`, inside `fn migrate()`, add the settings table creation after the v0.3.2 migration block (after line 173):

```rust
// v0.33 migration: settings table
conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );"
)?;
```

- [ ] **Step 2: Add settings CRUD methods**

Add the following methods to `impl Database` in `src/db.rs` (before the closing `}`):

```rust
// --- Settings CRUD ---

pub fn get_setting(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| row.get(0))?;
    Ok(rows.next().transpose()?)
}

pub fn set_setting(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params![key, value, now],
    )?;
    Ok(())
}

pub fn get_all_settings(&self) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

pub fn set_all_settings(&self, settings: &std::collections::HashMap<String, String>) -> Result<(), rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    for (key, value) in settings {
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![key, value, now],
        )?;
    }
    Ok(())
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add src/db.rs
git commit -m "feat(v0.33): add settings table and CRUD methods"
```

---

## Task 2: Config Refactoring

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/config.rs`
- Modify: `docker/server/config/config.toml`

- [ ] **Step 1: Add toml_edit dependency**

In `Cargo.toml`, add after the `toml = "0.8"` line:

```toml
toml_edit = "0.22"
```

- [ ] **Step 2: Add new config fields**

In `src/config.rs`, add two fields to `ServerConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub admin_password: String,
    #[serde(default = "default_min_trust_level")]
    pub min_trust_level: i32,
    #[serde(default = "default_false")]
    pub allow_registration: bool,
    #[serde(default = "default_true")]
    pub enable_oauth: bool,
}

fn default_false() -> bool { false }
fn default_true() -> bool { true }
```

- [ ] **Step 3: Add settings seed function**

Add a new public function in `src/config.rs` that seeds all runtime config values into the DB:

```rust
use crate::db::Database;

/// Seeds runtime config from config.toml into DB settings table.
/// Called at startup — config.toml always overwrites DB values.
pub fn seed_settings_to_db(db: &Database, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = std::collections::HashMap::new();

    // Server runtime settings
    settings.insert("admin_password".to_string(), config.server.admin_password.clone());
    settings.insert("min_trust_level".to_string(), config.server.min_trust_level.to_string());
    settings.insert("allow_registration".to_string(), config.server.allow_registration.to_string());
    settings.insert("enable_oauth".to_string(), config.server.enable_oauth.to_string());

    // OAuth settings
    settings.insert("oauth_client_id".to_string(), config.oauth.client_id.clone());
    settings.insert("oauth_client_secret".to_string(), config.oauth.client_secret.clone());
    settings.insert("oauth_redirect_uri".to_string(), config.oauth.redirect_uri.clone());

    // Singbox runtime settings
    settings.insert("singbox_api_secret".to_string(), config.singbox.api_secret.clone().unwrap_or_default());

    // Validation settings
    settings.insert("validation_url".to_string(), config.validation.url.clone());
    settings.insert("validation_timeout_secs".to_string(), config.validation.timeout_secs.to_string());
    settings.insert("validation_concurrency".to_string(), config.validation.concurrency.to_string());
    settings.insert("validation_interval_mins".to_string(), config.validation.interval_mins.to_string());
    settings.insert("validation_error_threshold".to_string(), config.validation.error_threshold.to_string());
    settings.insert("validation_batch_size".to_string(), config.validation.batch_size.to_string());

    // Quality settings
    settings.insert("quality_interval_mins".to_string(), config.quality.interval_mins.to_string());
    settings.insert("quality_concurrency".to_string(), config.quality.concurrency.to_string());

    // Subscription settings
    settings.insert("subscription_auto_refresh_interval_mins".to_string(), config.subscription.auto_refresh_interval_mins.to_string());

    db.set_all_settings(&settings)?;
    tracing::info!("Seeded {} settings from config.toml to DB", settings.len());
    Ok(())
}
```

- [ ] **Step 4: Add config writeback function**

Add a function that writes settings back to config.toml using `toml_edit`:

```rust
/// Writes runtime settings back to config.toml, preserving comments and formatting.
pub fn write_settings_to_config(
    settings: &std::collections::HashMap<String, String>,
    config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {config_path}: {e}"))?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("Failed to parse TOML: {e}"))?;

    // Map flat setting keys back to TOML sections
    for (key, value) in settings {
        match key.as_str() {
            "admin_password" => doc["server"]["admin_password"] = toml_edit::value(value.as_str()),
            "min_trust_level" => doc["server"]["min_trust_level"] = toml_edit::value(value.parse::<i64>().unwrap_or(1)),
            "allow_registration" => doc["server"]["allow_registration"] = toml_edit::value(value == "true"),
            "enable_oauth" => doc["server"]["enable_oauth"] = toml_edit::value(value == "true"),
            "oauth_client_id" => doc["oauth"]["client_id"] = toml_edit::value(value.as_str()),
            "oauth_client_secret" => doc["oauth"]["client_secret"] = toml_edit::value(value.as_str()),
            "oauth_redirect_uri" => doc["oauth"]["redirect_uri"] = toml_edit::value(value.as_str()),
            "singbox_api_secret" => doc["singbox"]["api_secret"] = toml_edit::value(value.as_str()),
            "validation_url" => doc["validation"]["url"] = toml_edit::value(value.as_str()),
            "validation_timeout_secs" => doc["validation"]["timeout_secs"] = toml_edit::value(value.parse::<i64>().unwrap_or(10)),
            "validation_concurrency" => doc["validation"]["concurrency"] = toml_edit::value(value.parse::<i64>().unwrap_or(50)),
            "validation_interval_mins" => doc["validation"]["interval_mins"] = toml_edit::value(value.parse::<i64>().unwrap_or(30)),
            "validation_error_threshold" => doc["validation"]["error_threshold"] = toml_edit::value(value.parse::<i64>().unwrap_or(10)),
            "validation_batch_size" => doc["validation"]["batch_size"] = toml_edit::value(value.parse::<i64>().unwrap_or(30)),
            "quality_interval_mins" => doc["quality"]["interval_mins"] = toml_edit::value(value.parse::<i64>().unwrap_or(120)),
            "quality_concurrency" => doc["quality"]["concurrency"] = toml_edit::value(value.parse::<i64>().unwrap_or(10)),
            "subscription_auto_refresh_interval_mins" => doc["subscription"]["auto_refresh_interval_mins"] = toml_edit::value(value.parse::<i64>().unwrap_or(0)),
            _ => { tracing::warn!("Unknown settings key for config writeback: {key}"); }
        }
    }

    std::fs::write(config_path, doc.to_string())
        .map_err(|e| format!("Failed to write {config_path}: {e}"))?;
    tracing::info!("Settings written back to {config_path}");
    Ok(())
}
```

- [ ] **Step 5: Update docker config.toml template**

In `docker/server/config/config.toml`, add the new fields to `[server]`:

```toml
[server]
host = "0.0.0.0"
port = 3000
admin_password = "change-me"
min_trust_level = 1
allow_registration = false
enable_oauth = true
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/config.rs docker/server/config/config.toml
git commit -m "feat(v0.33): config refactoring with seed/writeback functions"
```

---

## Task 3: AppState and Startup Flow

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add config_path to AppState and seed settings at startup**

In `src/main.rs`, add `config_path` field to `AppState`:

```rust
pub struct AppState {
    pub config: AppConfig,
    pub config_path: String,  // NEW: path to config.toml for writeback
    pub db: Database,
    pub pool: ProxyPool,
    pub singbox: Arc<Mutex<SingboxManager>>,
    pub relay_clients: DashMap<u16, reqwest::Client>,
    pub auth_cache: DashMap<String, (User, tokio::time::Instant)>,
    pub validation_lock: Mutex<()>,
}
```

- [ ] **Step 2: Update state initialization to include config_path and seed settings**

In `main()` function, after `db` is initialized (after line 47), add settings seed call:

```rust
// Seed runtime settings from config.toml to DB
if let Err(e) = crate::config::seed_settings_to_db(&db, &config) {
    tracing::error!("Failed to seed settings: {e}");
}
```

And in the `AppState` construction (around line 89), add the config_path field:

```rust
let state = Arc::new(AppState {
    config: config.clone(),
    config_path: "config.toml".to_string(),  // NEW
    db,
    pool,
    singbox,
    relay_clients: DashMap::new(),
    auth_cache: DashMap::new(),
    validation_lock: Mutex::new(()),
});
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(v0.33): add config_path to AppState, seed settings at startup"
```

---

## Task 4: Admin Settings API

**Files:**
- Modify: `src/api/admin.rs`
- Modify: `src/api/mod.rs`

- [ ] **Step 1: Add settings handlers to admin.rs**

Add to `src/api/admin.rs`:

```rust
use crate::config::write_settings_to_config;

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let settings = state.db.get_all_settings()?;
    Ok(Json(json!(settings)))
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateSettingsRequest(pub std::collections::HashMap<String, String>);

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // 1. Write to DB
    state.db.set_all_settings(&req.0)?;

    // 2. Write back to config.toml
    if let Err(e) = write_settings_to_config(&req.0, &state.config_path) {
        tracing::error!("Failed to write settings to config file: {e}");
        return Err(AppError::Internal(format!("Settings saved to DB but config file write failed: {e}")));
    }

    tracing::info!("Settings updated via admin UI ({} keys)", req.0.len());
    Ok(Json(json!({ "message": "Settings saved" })))
}
```

- [ ] **Step 2: Register settings routes**

In `src/api/mod.rs`, add the settings routes to the admin routes block (after line 44, before `.route_layer`):

```rust
.route("/api/admin/settings", get(admin::get_settings).put(admin::update_settings))
```

- [ ] **Step 3: Update admin_auth middleware to read password from DB**

In `src/api/mod.rs`, update the `admin_auth` function to read `admin_password` from DB:

```rust
async fn admin_auth(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Read admin_password from DB settings (runtime), fallback to config
    let expected = state.db.get_setting("admin_password")
        .ok()
        .flatten()
        .unwrap_or_else(|| state.config.server.admin_password.clone());

    let authorized = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == expected)
        .unwrap_or(false);

    if authorized {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add src/api/admin.rs src/api/mod.rs
git commit -m "feat(v0.33): admin settings API (GET/PUT) and DB-backed admin auth"
```

---

## Task 5: Auth Options and Registration

**Files:**
- Modify: `src/api/auth.rs`
- Modify: `src/api/mod.rs`

- [ ] **Step 1: Add auth_options handler**

Add to `src/api/auth.rs`:

```rust
/// Public endpoint — returns which login/register methods are available.
/// Used by the login page JS to dynamically render the UI.
pub async fn auth_options(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let enable_oauth = state.db.get_setting("enable_oauth")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(true);
    let allow_registration = state.db.get_setting("allow_registration")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(false);

    Json(json!({
        "enable_oauth": enable_oauth,
        "allow_registration": allow_registration,
    }))
}
```

- [ ] **Step 2: Add register handler**

Add to `src/api/auth.rs`:

```rust
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
    use argon2::password_hash::{SaltString, rand_core::OsRng};
    use argon2::PasswordHasher;
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Hash error: {e}")))?
        .to_string();

    let min_trust = state.db.get_setting("min_trust_level")
        .ok().flatten()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(1);

    let user = state.db.create_password_user(&req.username, &hash, min_trust)?;

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
```

- [ ] **Step 3: Add Forbidden, BadRequest, and Conflict error variants**

In `src/error.rs`, check if `Forbidden`, `BadRequest`, and `Conflict` variants exist. If not, add them:

```rust
pub enum AppError {
    Internal(String),
    Unauthorized(String),
    Forbidden(String),    // NEW - 403
    BadRequest(String),   // NEW - 400
    Conflict(String),     // NEW - 409
}
```

And their `IntoResponse` implementations returning the appropriate status codes.

- [ ] **Step 4: Add OAuth toggle to login handler**

In `src/api/auth.rs`, at the top of the `login` function, add:

```rust
pub async fn login(State(state): State<Arc<AppState>>) -> Response {
    // Check if OAuth is enabled
    let enabled = state.db.get_setting("enable_oauth")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(true);
    if !enabled {
        return (axum::http::StatusCode::FORBIDDEN, "OAuth login is disabled").into_response();
    }

    let client_id = &state.config.oauth.client_id;
    // ... rest unchanged
```

- [ ] **Step 5: Add OAuth toggle to callback handler**

In `src/api/auth.rs`, at the top of the `callback` function, add:

```rust
pub async fn callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    // Check if OAuth is enabled
    let enabled = state.db.get_setting("enable_oauth")
        .ok().flatten()
        .map(|v| v == "true")
        .unwrap_or(true);
    if !enabled {
        return Err(AppError::Forbidden("OAuth login is disabled".into()));
    }

    // ... rest unchanged
```

- [ ] **Step 6: Update OAuth config reads to use DB settings**

In `src/api/auth.rs`, update the `login` function to read OAuth config from DB:

```rust
// Replace state.config.oauth.client_id with DB reads
let client_id = state.db.get_setting("oauth_client_id")
    .ok().flatten()
    .unwrap_or_else(|| state.config.oauth.client_id.clone());
let redirect_uri = state.db.get_setting("oauth_redirect_uri")
    .ok().flatten()
    .unwrap_or_else(|| state.config.oauth.redirect_uri.clone());
```

Apply the same pattern in `callback` for `client_id`, `client_secret`, and `redirect_uri`.

Also update `min_trust` read in `callback` (line 97):

```rust
let min_trust = state.db.get_setting("min_trust_level")
    .ok().flatten()
    .and_then(|v| v.parse::<i32>().ok())
    .unwrap_or(state.config.server.min_trust_level);
```

- [ ] **Step 7: Register new routes**

In `src/api/mod.rs`, add to the auth routes:

```rust
.route("/api/auth/options", get(auth::auth_options))
.route("/api/auth/register", post(auth::register))
```

- [ ] **Step 8: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors.

- [ ] **Step 9: Commit**

```bash
git add src/api/auth.rs src/api/mod.rs src/error.rs
git commit -m "feat(v0.33): auth options, registration endpoint, OAuth toggle"
```

---

## Task 6: Runtime Config Reads Migration

**Files:**
- Modify: `src/pool/validator.rs`
- Modify: `src/quality/checker.rs`
- Modify: `src/api/admin.rs`
- Modify: `src/api/subscription.rs`
- Modify: `src/main.rs`

This task migrates all `state.config.*` reads for runtime settings to read from DB instead.

- [ ] **Step 1: Update validator.rs**

In `src/pool/validator.rs`, update the `validate_all` function. Replace lines 33-36:

```rust
// Before:
let concurrency = state.config.validation.concurrency;
let timeout_duration = std::time::Duration::from_secs(state.config.validation.timeout_secs);
let validation_url = state.config.validation.url.clone();
let max_proxies = state.config.singbox.max_proxies;

// After:
let concurrency = state.db.get_setting("validation_concurrency")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.validation.concurrency);
let timeout_secs = state.db.get_setting("validation_timeout_secs")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.validation.timeout_secs);
let timeout_duration = std::time::Duration::from_secs(timeout_secs);
let validation_url = state.db.get_setting("validation_url")
    .ok().flatten()
    .unwrap_or_else(|| state.config.validation.url.clone());
let max_proxies = state.config.singbox.max_proxies; // boot config, stays as-is
```

Also update line 109 (`error_threshold`):

```rust
let threshold = state.db.get_setting("validation_error_threshold")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.validation.error_threshold);
```

- [ ] **Step 2: Update checker.rs**

In `src/quality/checker.rs`, update line 119:

```rust
// Before:
let semaphore = Arc::new(Semaphore::new(state.config.quality.concurrency));

// After:
let quality_concurrency = state.db.get_setting("quality_concurrency")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.quality.concurrency);
let semaphore = Arc::new(Semaphore::new(quality_concurrency));
```

- [ ] **Step 3: Update admin.rs runtime config reads**

In `src/api/admin.rs`, update line 59 (`cleanup_proxies`):

```rust
let threshold = state.db.get_setting("validation_error_threshold")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.validation.error_threshold);
```

Update line 175 (`create_password_user`):

```rust
let trust_level = state.db.get_setting("min_trust_level")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.server.min_trust_level);
```

- [ ] **Step 4: Update subscription.rs runtime config reads**

In `src/api/subscription.rs`, update lines 341-342:

```rust
let max = state.config.singbox.max_proxies; // boot config, stays
let batch = state.db.get_setting("validation_batch_size")
    .ok().flatten().and_then(|v| v.parse().ok())
    .unwrap_or(state.config.validation.batch_size);
```

- [ ] **Step 5: Update main.rs background tasks to re-read from DB**

In `src/main.rs`, update the periodic validation task (around line 123-132):

```rust
tokio::spawn(async move {
    loop {
        let interval_mins: u64 = state_clone.db.get_setting("validation_interval_mins")
            .ok().flatten().and_then(|v| v.parse().ok())
            .unwrap_or(state_clone.config.validation.interval_mins);
        tokio::time::sleep(std::time::Duration::from_secs(interval_mins * 60)).await;
        tracing::info!("Running periodic proxy validation...");
        if let Err(e) = pool::validator::validate_all(state_clone.clone()).await {
            tracing::error!("Validation error: {e}");
        }
    }
});
```

Update the subscription auto-refresh task (around line 183-194) similarly — read `subscription_auto_refresh_interval_mins` from DB each loop iteration instead of checking once at startup.

```rust
// Remove the `if state.config.subscription.auto_refresh_interval_mins > 0` guard.
// Instead, always spawn the task and check the interval inside the loop:
let state_clone = state.clone();
tokio::spawn(async move {
    loop {
        let interval_mins: u64 = state_clone.db.get_setting("subscription_auto_refresh_interval_mins")
            .ok().flatten().and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if interval_mins == 0 {
            // Disabled; check again in 5 minutes
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            continue;
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval_mins * 60)).await;
        refresh_all_subscriptions(&state_clone).await;
    }
});
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add src/pool/validator.rs src/quality/checker.rs src/api/admin.rs src/api/subscription.rs src/main.rs
git commit -m "feat(v0.33): migrate runtime config reads from config struct to DB"
```

---

## Task 7: Admin UI — Settings Panel

**Files:**
- Modify: `src/web/admin.html`

- [ ] **Step 1: Add Settings section HTML**

In `src/web/admin.html`, insert a new section **between** the "操作" section (line ~140) and the "用户管理" section (line ~142). Add:

```html
  <!-- Settings -->
  <div class="section">
    <div class="section-title">系统配置 <button class="btn btn-primary btn-sm" onclick="saveSettings()" id="save-settings-btn" style="display:none">保存配置</button></div>
    <div class="card">
      <div class="grid-2">
        <div>
          <h3 style="font-size:14px;color:var(--text-bright);margin-bottom:12px">认证设置</h3>
          <div class="form-group"><label>管理员密码</label><input id="s-admin_password" type="password" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>最低信任等级</label><input id="s-min_trust_level" type="number" min="0" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label style="display:flex;align-items:center;gap:8px"><input type="checkbox" id="s-allow_registration" onchange="markSettingsDirty()" style="width:auto;accent-color:var(--primary)"> 允许用户自助注册</label></div>
          <div class="form-group"><label style="display:flex;align-items:center;gap:8px"><input type="checkbox" id="s-enable_oauth" onchange="markSettingsDirty()" style="width:auto;accent-color:var(--primary)"> 启用 OAuth 登录</label></div>
        </div>
        <div>
          <h3 style="font-size:14px;color:var(--text-bright);margin-bottom:12px">OAuth 配置</h3>
          <div class="form-group"><label>Client ID</label><input id="s-oauth_client_id" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>Client Secret</label><input id="s-oauth_client_secret" type="password" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>Redirect URI</label><input id="s-oauth_redirect_uri" oninput="markSettingsDirty()"></div>
        </div>
      </div>
      <div class="grid-2" style="margin-top:16px">
        <div>
          <h3 style="font-size:14px;color:var(--text-bright);margin-bottom:12px">验证配置</h3>
          <div class="form-group"><label>验证 URL</label><input id="s-validation_url" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>超时 (秒)</label><input id="s-validation_timeout_secs" type="number" min="1" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>并发数</label><input id="s-validation_concurrency" type="number" min="1" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>间隔 (分钟)</label><input id="s-validation_interval_mins" type="number" min="1" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>错误阈值</label><input id="s-validation_error_threshold" type="number" min="1" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>批量大小</label><input id="s-validation_batch_size" type="number" min="1" oninput="markSettingsDirty()"></div>
        </div>
        <div>
          <h3 style="font-size:14px;color:var(--text-bright);margin-bottom:12px">质检 / 订阅配置</h3>
          <div class="form-group"><label>质检间隔 (分钟)</label><input id="s-quality_interval_mins" type="number" min="1" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>质检并发数</label><input id="s-quality_concurrency" type="number" min="1" oninput="markSettingsDirty()"></div>
          <div class="form-group"><label>订阅自动刷新间隔 (分钟, 0=关闭)</label><input id="s-subscription_auto_refresh_interval_mins" type="number" min="0" oninput="markSettingsDirty()"></div>
        </div>
      </div>
      <div style="margin-top:16px;display:flex;align-items:center;justify-content:space-between">
        <span id="settings-status" style="font-size:13px;color:var(--text-dim)"></span>
        <button class="btn btn-primary" onclick="saveSettings()" id="save-settings-btn-bottom" style="display:none">保存配置</button>
      </div>
    </div>
  </div>
```

- [ ] **Step 2: Add Settings JavaScript**

In `src/web/admin.html`, add the following JS functions inside the `<script>` block:

```javascript
// --- Settings ---
let settingsDirty = false;

const SETTING_KEYS = [
  'admin_password', 'min_trust_level', 'allow_registration', 'enable_oauth',
  'oauth_client_id', 'oauth_client_secret', 'oauth_redirect_uri',
  'validation_url', 'validation_timeout_secs', 'validation_concurrency',
  'validation_interval_mins', 'validation_error_threshold', 'validation_batch_size',
  'quality_interval_mins', 'quality_concurrency', 'subscription_auto_refresh_interval_mins'
];

const CHECKBOX_KEYS = ['allow_registration', 'enable_oauth'];

async function loadSettings() {
  const d = await api('/api/admin/settings');
  if (!d) return;
  for (const key of SETTING_KEYS) {
    const el = document.getElementById('s-' + key);
    if (!el) continue;
    if (CHECKBOX_KEYS.includes(key)) {
      el.checked = d[key] === 'true';
    } else {
      el.value = d[key] || '';
    }
  }
  settingsDirty = false;
  updateSettingsUI();
}

function markSettingsDirty() {
  settingsDirty = true;
  updateSettingsUI();
}

function updateSettingsUI() {
  const status = document.getElementById('settings-status');
  const btn = document.getElementById('save-settings-btn');
  const btnBottom = document.getElementById('save-settings-btn-bottom');
  if (settingsDirty) {
    status.textContent = '● 有未保存的修改';
    status.style.color = 'var(--warn)';
    btn.style.display = '';
    btnBottom.style.display = '';
  } else {
    status.textContent = '✅ 配置已同步';
    status.style.color = 'var(--success)';
    btn.style.display = 'none';
    btnBottom.style.display = 'none';
  }
}

async function saveSettings() {
  const settings = {};
  for (const key of SETTING_KEYS) {
    const el = document.getElementById('s-' + key);
    if (!el) continue;
    if (CHECKBOX_KEYS.includes(key)) {
      settings[key] = el.checked ? 'true' : 'false';
    } else {
      settings[key] = el.value;
    }
  }
  const r = await fetch('/api/admin/settings', {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(settings)
  });
  if (r.ok) {
    toast('✅ 配置已保存');
    settingsDirty = false;
    updateSettingsUI();
    // If admin password changed, update stored password
    if (settings.admin_password && settings.admin_password !== getPassword()) {
      localStorage.setItem(STORAGE_KEY, settings.admin_password);
    }
  } else {
    const d = await r.json().catch(() => ({}));
    toast('保存失败: ' + (d.error || r.statusText));
  }
}

// Add beforeunload warning
window.addEventListener('beforeunload', (e) => {
  if (settingsDirty) {
    e.preventDefault();
    e.returnValue = '';
  }
});
```

- [ ] **Step 3: Add loadSettings to refresh function**

Update the `refresh()` function:

```javascript
function refresh() { loadStats(); loadUsers(); loadSubscriptions(); loadProxies(); loadSettings(); }
```

- [ ] **Step 4: Verify admin.html renders correctly (manual check after full build)**

This will be verified during the integration test in Task 9.

- [ ] **Step 5: Commit**

```bash
git add src/web/admin.html
git commit -m "feat(v0.33): admin UI settings panel with save button"
```

---

## Task 8: Login Page Restructuring

**Files:**
- Modify: `src/web/user.html`

- [ ] **Step 1: Restructure login overlay HTML**

In `src/web/user.html`, replace the entire login-overlay div (lines 92-107) with:

```html
<!-- Login Screen -->
<div class="login-overlay" id="login-overlay">
  <div class="login-box" id="login-view">
    <h2>ZenProxy</h2>
    <p>选择登录方式</p>
    <div id="pwd-login-error" style="color:var(--danger);font-size:13px;margin-bottom:8px;display:none"></div>
    <input type="text" id="pwd-username" placeholder="用户名" style="width:100%;padding:12px 16px;background:#0f1117;border:1px solid var(--border);border-radius:8px;color:var(--text);font-size:14px;outline:none;margin-bottom:10px">
    <input type="password" id="pwd-password" placeholder="密码" style="width:100%;padding:12px 16px;background:#0f1117;border:1px solid var(--border);border-radius:8px;color:var(--text);font-size:14px;outline:none;margin-bottom:16px" onkeydown="if(event.key==='Enter')doPasswordLogin()">
    <button class="btn btn-primary" onclick="doPasswordLogin()" style="width:100%;justify-content:center;margin-bottom:0">密码登录</button>
    <div id="register-link" style="display:none;margin-top:12px;text-align:center"><a href="javascript:void(0)" onclick="showRegisterView()" style="color:var(--primary);font-size:13px;text-decoration:none">注册新账号</a></div>
    <div id="oauth-section" style="display:none">
      <div style="display:flex;align-items:center;gap:12px;margin:16px 0;color:var(--text-dim);font-size:13px">
        <div style="flex:1;height:1px;background:var(--border)"></div>
        <span>或</span>
        <div style="flex:1;height:1px;background:var(--border)"></div>
      </div>
      <a class="btn btn-primary" href="/api/auth/login" style="width:100%;justify-content:center;text-decoration:none;background:transparent;border:1px solid var(--primary);color:var(--primary)">使用 Linux DO 登录</a>
    </div>
  </div>
  <div class="login-box" id="register-view" style="display:none">
    <h2>ZenProxy</h2>
    <p>注册新账号</p>
    <div id="reg-error" style="color:var(--danger);font-size:13px;margin-bottom:8px;display:none"></div>
    <input type="text" id="reg-username" placeholder="用户名" style="width:100%;padding:12px 16px;background:#0f1117;border:1px solid var(--border);border-radius:8px;color:var(--text);font-size:14px;outline:none;margin-bottom:10px">
    <input type="password" id="reg-password" placeholder="密码" style="width:100%;padding:12px 16px;background:#0f1117;border:1px solid var(--border);border-radius:8px;color:var(--text);font-size:14px;outline:none;margin-bottom:10px">
    <input type="password" id="reg-password2" placeholder="确认密码" style="width:100%;padding:12px 16px;background:#0f1117;border:1px solid var(--border);border-radius:8px;color:var(--text);font-size:14px;outline:none;margin-bottom:16px" onkeydown="if(event.key==='Enter')doRegister()">
    <button class="btn btn-primary" onclick="doRegister()" style="width:100%;justify-content:center;margin-bottom:12px">注册</button>
    <a href="javascript:void(0)" onclick="showLoginView()" style="color:var(--text-dim);font-size:13px;text-decoration:none">已有账号？返回登录</a>
  </div>
</div>
```

- [ ] **Step 2: Add dynamic rendering and registration JS**

In `src/web/user.html`, update the `checkAuth` function and add new functions:

```javascript
async function checkAuth() {
  try {
    const r = await fetch('/api/auth/me');
    if (r.status === 401) {
      document.getElementById('login-overlay').style.display = '';
      document.getElementById('app').style.display = 'none';
      loadAuthOptions();
      return;
    }
    currentUser = await r.json();
    document.getElementById('login-overlay').style.display = 'none';
    document.getElementById('app').style.display = '';
    document.getElementById('header-username').textContent = currentUser.username;
    updateKeyDisplay();
    refresh();
  } catch(e) {
    document.getElementById('login-overlay').style.display = '';
    document.getElementById('app').style.display = 'none';
    loadAuthOptions();
  }
}

async function loadAuthOptions() {
  try {
    const r = await fetch('/api/auth/options');
    const opts = await r.json();
    document.getElementById('oauth-section').style.display = opts.enable_oauth ? '' : 'none';
    document.getElementById('register-link').style.display = opts.allow_registration ? '' : 'none';
  } catch(e) {
    // Defaults: hide both if can't fetch options
  }
}

function showRegisterView() {
  document.getElementById('login-view').style.display = 'none';
  document.getElementById('register-view').style.display = '';
}

function showLoginView() {
  document.getElementById('register-view').style.display = 'none';
  document.getElementById('login-view').style.display = '';
}

async function doRegister() {
  const username = document.getElementById('reg-username').value.trim();
  const password = document.getElementById('reg-password').value;
  const password2 = document.getElementById('reg-password2').value;
  const errEl = document.getElementById('reg-error');
  if (!username || !password) { errEl.textContent = '请输入用户名和密码'; errEl.style.display = 'block'; return; }
  if (password !== password2) { errEl.textContent = '两次输入的密码不一致'; errEl.style.display = 'block'; return; }
  try {
    const r = await fetch('/api/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password })
    });
    if (r.ok) {
      location.reload();
    } else {
      const d = await r.json().catch(() => ({}));
      errEl.textContent = d.error || '注册失败';
      errEl.style.display = 'block';
    }
  } catch(e) {
    errEl.textContent = '注册失败: ' + e.message;
    errEl.style.display = 'block';
  }
}
```

- [ ] **Step 3: Commit**

```bash
git add src/web/user.html
git commit -m "feat(v0.33): restructure login page with dynamic OAuth/registration display"
```

---

## Task 9: Docker Compose Updates and Integration Test

**Files:**
- Modify: `docker/server/docker-compose.yml`
- Modify: `docker/server/docker-compose-remote.yml`
- Modify: `docker/server/.env`

- [ ] **Step 1: Remove :ro from config.toml mount in docker-compose.yml**

In `docker/server/docker-compose.yml`, change line 10:

```yaml
# Before:
      - ./config/config.toml:/app/config.toml:ro
# After:
      - ./config/config.toml:/app/config.toml
```

- [ ] **Step 2: Remove :ro from config.toml mount in docker-compose-remote.yml**

In `docker/server/docker-compose-remote.yml`, change line 8:

```yaml
# Before:
      - ./config/config.toml:/app/config.toml:ro
# After:
      - ./config/config.toml:/app/config.toml
```

- [ ] **Step 3: Clean up .env — remove unused DOCKERHUB_USERNAME and IMAGE_TAG**

In `docker/server/.env`, remove the `DOCKERHUB_USERNAME` and `IMAGE_TAG` lines (they are unused):

```env
# Port mapping
SERVER_PORT=3000
SINGBOX_API_PORT=9090
PROXY_PORT_START=10002
PROXY_PORT_END=10301

# Logging
RUST_LOG=zenproxy=info,tower_http=info
```

- [ ] **Step 4: Full build verification**

Run: `cargo build`
Expected: Builds without errors.

- [ ] **Step 5: Commit all remaining changes**

```bash
git add docker/server/docker-compose.yml docker/server/docker-compose-remote.yml docker/server/.env
git commit -m "chore(v0.33): docker compose updates, remove :ro, clean up .env"
```

- [ ] **Step 6: Local smoke test**

1. Start the server: `cargo run` (from project root, with a config.toml present)
2. Open `http://localhost:3000` — verify login page shows password login at top, OAuth below
3. Open `http://localhost:3000/admin` — verify Settings panel appears with all fields populated
4. Modify a setting in the panel, click "保存配置" — verify toast shows success
5. Check `config.toml` on disk — verify the value was written back
6. Restart the server — verify the changed value persists (config.toml seeds back to DB)

- [ ] **Step 7: Final commit — update design status**

Update `docs/dev_notes/v0.33/design.md` status line:

```markdown
> **状态**：implementation complete，待测试验证。
```

```bash
git add docs/dev_notes/v0.33/design.md
git commit -m "docs(v0.33): mark implementation complete"
```

---

## Summary

| Task | Description | Est. Lines |
|---|---|---|
| 1 | DB settings table + CRUD | ~60 |
| 2 | Config refactoring (new fields, seed, writeback) | ~150 |
| 3 | AppState + startup flow | ~15 |
| 4 | Admin settings API + auth middleware update | ~60 |
| 5 | Auth options, registration, OAuth toggle | ~100 |
| 6 | Runtime config reads migration | ~40 |
| 7 | Admin UI settings panel | ~120 |
| 8 | Login page restructuring | ~80 |
| 9 | Docker compose + integration test | ~10 |
| **Total** | | **~635** |
