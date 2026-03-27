# Task 01: DB Schema + Config Foundation

**Depends on:** None
**Blocking:** T02, T03, T04, T05, T06

---

## Goal

Add `role` column to users table, `is_disabled` column to proxies table, remove `admin_password` from config, rename OAuth settings keys to `linuxdo_*` prefix, and update all struct/seeding/writeback code.

## Files

- Modify: `src/db.rs` (migrations + struct + CRUD updates)
- Modify: `src/config.rs` (struct changes + seed/writeback rename)
- Modify: `docker/server/config/config.toml` (template cleanup)

---

## Steps

- [ ] **Step 1: Add `role` column migration to `db.rs`**

In `Database::migrate()`, after the v0.33 settings table migration (line ~182), add:

```rust
// v0.35 migration: add role column to users
let has_role: bool = conn
    .prepare("SELECT COUNT(*) FROM pragma_table_info('users') WHERE name='role'")?
    .query_row([], |r| r.get::<_, i32>(0))
    .map(|c| c > 0)?;
if !has_role {
    conn.execute_batch(
        "ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';"
    )?;
}
```

- [ ] **Step 2: Add `is_disabled` column migration to `db.rs`**

Immediately after step 1's migration block:

```rust
// v0.35 migration: add is_disabled column to proxies
let has_disabled: bool = conn
    .prepare("SELECT COUNT(*) FROM pragma_table_info('proxies') WHERE name='is_disabled'")?
    .query_row([], |r| r.get::<_, i32>(0))
    .map(|c| c > 0)?;
if !has_disabled {
    conn.execute_batch(
        "ALTER TABLE proxies ADD COLUMN is_disabled INTEGER NOT NULL DEFAULT 0;"
    )?;
}
```

- [ ] **Step 3: Update `User` struct**

In `src/db.rs`, add `role` field to the `User` struct (after `auth_source`):

```rust
pub struct User {
    // ... existing fields ...
    pub auth_source: String,
    pub role: String,  // "user" | "admin" | "super_admin"
}
```

- [ ] **Step 4: Update `ProxyRow` struct**

Add `is_disabled` field:

```rust
pub struct ProxyRow {
    // ... existing fields ...
    pub updated_at: String,
    pub is_disabled: bool,
}
```

- [ ] **Step 5: Update all user query methods in `db.rs`**

Every method that reads users must include `role` in SELECT and mapping. Update:
- `upsert_user` — INSERT/UPDATE to include `role` (default 'user' for OAuth users)
- `get_user_by_id` — add `role` to SELECT and struct construction
- `get_user_by_username` — same
- `get_user_by_api_key` — same
- `get_all_users` — same
- `create_password_user` — add `role` param (default 'user')

Add new methods:
```rust
pub fn update_user_role(&self, id: &str, role: &str) -> Result<(), rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE users SET role = ?1, updated_at = ?2 WHERE id = ?3",
        params![role, now, id],
    )?;
    Ok(())
}

pub fn count_users_by_role(&self, role: &str) -> Result<i32, rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM users WHERE role = ?1",
        params![role],
        |r| r.get(0),
    )
}
```

- [ ] **Step 6: Update proxy query methods in `db.rs`**

Update `get_all_proxies` and `insert_proxy` to include `is_disabled`. Add:

```rust
pub fn set_proxy_disabled(&self, id: &str, disabled: bool) -> Result<(), rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE proxies SET is_disabled = ?1, updated_at = ?2 WHERE id = ?3",
        params![disabled as i32, now, id],
    )?;
    Ok(())
}
```

Also add subscription update method:
```rust
pub fn update_subscription(&self, id: &str, name: &str, url: Option<&str>) -> Result<(), rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE subscriptions SET name = ?1, url = ?2, updated_at = ?3 WHERE id = ?4",
        params![name, url, now, id],
    )?;
    Ok(())
}
```

- [ ] **Step 7: Update `ServerConfig` in `config.rs`**

Remove `admin_password`, `min_trust_level`, `enable_oauth` from `ServerConfig`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_false")]
    pub allow_registration: bool,
}
```

- [ ] **Step 8: Update `OAuthConfig` in `config.rs`**

Restructure to support provider nesting:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthConfig {
    pub linuxdo: LinuxDoOAuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinuxDoOAuthConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default = "default_min_trust_level")]
    pub min_trust_level: i32,
}
```

- [ ] **Step 9: Update `seed_settings_to_db` in `config.rs`**

Replace all old key names with new ones:

```rust
pub fn seed_settings_to_db(db: &Database, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = std::collections::HashMap::new();

    // Server settings (no more admin_password)
    settings.insert("allow_registration".to_string(), config.server.allow_registration.to_string());

    // Linux.do OAuth settings (renamed keys)
    settings.insert("linuxdo_oauth_enabled".to_string(), config.oauth.linuxdo.enabled.to_string());
    settings.insert("linuxdo_client_id".to_string(), config.oauth.linuxdo.client_id.clone());
    settings.insert("linuxdo_client_secret".to_string(), config.oauth.linuxdo.client_secret.clone());
    settings.insert("linuxdo_redirect_uri".to_string(), config.oauth.linuxdo.redirect_uri.clone());
    settings.insert("linuxdo_min_trust_level".to_string(), config.oauth.linuxdo.min_trust_level.to_string());

    // ... rest unchanged (singbox, validation, quality, subscription) ...
    // Remove the admin_password and old oauth lines
}
```

- [ ] **Step 10: Update `write_settings_to_config` in `config.rs`**

Update the match arms to use new key names and new TOML paths:

```rust
// Remove these old arms:
// "admin_password" => ...
// "min_trust_level" => ...
// "enable_oauth" => ...
// "oauth_client_id" => ...
// "oauth_client_secret" => ...
// "oauth_redirect_uri" => ...

// Add new arms:
"linuxdo_oauth_enabled" => doc["oauth"]["linuxdo"]["enabled"] = toml_edit::value(value == "true"),
"linuxdo_client_id" => doc["oauth"]["linuxdo"]["client_id"] = toml_edit::value(value.as_str()),
"linuxdo_client_secret" => doc["oauth"]["linuxdo"]["client_secret"] = toml_edit::value(value.as_str()),
"linuxdo_redirect_uri" => doc["oauth"]["linuxdo"]["redirect_uri"] = toml_edit::value(value.as_str()),
"linuxdo_min_trust_level" => doc["oauth"]["linuxdo"]["min_trust_level"] = toml_edit::value(value.parse::<i64>().unwrap_or(1)),
```

- [ ] **Step 11: Update `docker/server/config/config.toml`**

```toml
[server]
host = "0.0.0.0"
port = 3000
allow_registration = false

[oauth.linuxdo]
enabled = true
client_id = ""
client_secret = ""
redirect_uri = "https://your-domain.com/api/auth/callback"
min_trust_level = 1

# ... rest unchanged ...
```

- [ ] **Step 12: Update `auth.rs` to use new settings keys**

In `login`, `callback`, `me`, and `auth_options` functions, replace:
- `"enable_oauth"` → `"linuxdo_oauth_enabled"`
- `"oauth_client_id"` → `"linuxdo_client_id"`
- `"oauth_client_secret"` → `"linuxdo_client_secret"`
- `"oauth_redirect_uri"` → `"linuxdo_redirect_uri"`
- `"min_trust_level"` → `"linuxdo_min_trust_level"`
- Config fallbacks: `state.config.oauth.client_id` → `state.config.oauth.linuxdo.client_id` etc.

Also add `role` to the `me` endpoint response:
```rust
Ok(Json(json!({
    // ... existing fields ...
    "role": user.role,
})))
```

And add `role` to the `auth_options` response:
```rust
// Add to auth_options so the frontend knows the oauth provider name
Json(json!({
    "linuxdo_oauth_enabled": enable_oauth,
    "allow_registration": allow_registration,
}))
```

- [ ] **Step 13: Update OAuth user creation in `auth.rs` callback**

The `User` struct creation in callback (line ~170) needs `role` field:
```rust
let user = User {
    // ... existing fields ...
    auth_source: "oauth".to_string(),
    role: "user".to_string(),  // OAuth users are always "user" by default
};
```

Also update the `register` function's `create_password_user` call — the DB method now needs to accept role.

- [ ] **Step 14: Compile check**

Run: `cargo build 2>&1 | head -50`
Expected: Compiles successfully (or only warnings).

If errors: fix all references to removed `admin_password` / old OAuth config paths. Search with:
```bash
grep -rn "admin_password\|config\.server\.min_trust_level\|config\.server\.enable_oauth\|config\.oauth\.client_id\|config\.oauth\.client_secret\|config\.oauth\.redirect_uri" src/
```

- [ ] **Commit**

```bash
git add -A
git commit -m "feat(v0.35): DB schema + config foundation — role column, is_disabled, linuxdo-prefixed settings keys"
```
