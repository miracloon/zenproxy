# Task 02: Admin Module Split

**Depends on:** T01 (struct changes must exist)
**Blocking:** T03, T04, T05, T06

---

## Goal

Split `src/api/admin.rs` (244 lines) into a module directory with three focused files. Pure structural refactor — zero logic changes.

## Files

- Delete: `src/api/admin.rs`
- Create: `src/api/admin/mod.rs`
- Create: `src/api/admin/users.rs`
- Create: `src/api/admin/proxies.rs`
- Create: `src/api/admin/settings.rs`
- Modify: `src/api/mod.rs` (update `mod admin` — should resolve automatically since it's already `pub mod admin`)

---

## Steps

- [ ] **Step 1: Create `src/api/admin/` directory**

```bash
mkdir -p src/api/admin
```

- [ ] **Step 2: Create `src/api/admin/proxies.rs`**

Move these handlers from admin.rs:
- `list_proxies`
- `delete_proxy`
- `cleanup_proxies`
- `trigger_validation`
- `trigger_quality_check`

```rust
use crate::error::AppError;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

pub async fn list_proxies(/* ... */) -> Result<Json<serde_json::Value>, AppError> { /* exact copy */ }
pub async fn delete_proxy(/* ... */) -> Result<Json<serde_json::Value>, AppError> { /* exact copy */ }
pub async fn cleanup_proxies(/* ... */) -> Result<Json<serde_json::Value>, AppError> { /* exact copy */ }
pub async fn trigger_validation(/* ... */) -> Result<Json<serde_json::Value>, AppError> { /* exact copy */ }
pub async fn trigger_quality_check(/* ... */) -> Result<Json<serde_json::Value>, AppError> { /* exact copy */ }
```

- [ ] **Step 3: Create `src/api/admin/users.rs`**

Move these handlers:
- `list_users`
- `delete_user`
- `ban_user`
- `unban_user`
- `CreatePasswordUserRequest` struct
- `create_password_user`
- `ResetPasswordRequest` struct
- `reset_user_password`

```rust
use crate::error::AppError;
use crate::AppState;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHasher};
use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;
use std::sync::Arc;

// All handlers exact copy from admin.rs
```

- [ ] **Step 4: Create `src/api/admin/settings.rs`**

Move these handlers:
- `get_settings`
- `update_settings`
- `get_stats`

```rust
use crate::config::write_settings_to_config;
use crate::error::AppError;
use crate::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

// All handlers exact copy from admin.rs
```

- [ ] **Step 5: Create `src/api/admin/mod.rs`**

```rust
pub mod proxies;
pub mod settings;
pub mod users;
```

- [ ] **Step 6: Update `src/api/mod.rs` route references**

Change all `admin::` references to use the sub-module paths:

```rust
// Old:
.route("/api/admin/proxies", get(admin::list_proxies))
// New:
.route("/api/admin/proxies", get(admin::proxies::list_proxies))
```

Full mapping:
- `admin::list_proxies` → `admin::proxies::list_proxies`
- `admin::delete_proxy` → `admin::proxies::delete_proxy`
- `admin::cleanup_proxies` → `admin::proxies::cleanup_proxies`
- `admin::trigger_validation` → `admin::proxies::trigger_validation`
- `admin::trigger_quality_check` → `admin::proxies::trigger_quality_check`
- `admin::get_stats` → `admin::settings::get_stats`
- `admin::list_users` → `admin::users::list_users`
- `admin::delete_user` → `admin::users::delete_user`
- `admin::ban_user` → `admin::users::ban_user`
- `admin::unban_user` → `admin::users::unban_user`
- `admin::create_password_user` → `admin::users::create_password_user`
- `admin::reset_user_password` → `admin::users::reset_user_password`
- `admin::get_settings` → `admin::settings::get_settings`
- `admin::update_settings` → `admin::settings::update_settings`

- [ ] **Step 7: Delete old `src/api/admin.rs`**

```bash
rm src/api/admin.rs
```

- [ ] **Step 8: Compile check + commit**

Run: `cargo build 2>&1 | head -30`
Expected: Compiles without errors. No logic changes, just file moves.

```bash
git add -A
git commit -m "refactor(v0.35): split admin.rs into admin/{mod,proxies,users,settings}.rs"
```
