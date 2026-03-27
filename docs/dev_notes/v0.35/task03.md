# Task 03: RBAC Auth + Super Admin Initialization

**Depends on:** T01, T02
**Blocking:** T07, T08

---

## Goal

Replace the `admin_password` Bearer token auth with session-based RBAC middleware. Add default super_admin initialization. Add role management API endpoints.

## Files

- Modify: `src/api/mod.rs` (rewrite `admin_auth` middleware)
- Modify: `src/main.rs` (add super_admin init at startup)
- Modify: `src/api/admin/users.rs` (add role change + permission-checked delete)
- Modify: `src/api/auth.rs` (may need small adjustments)

---

## Steps

- [ ] **Step 1: Rewrite `admin_auth` middleware in `src/api/mod.rs`**

Replace the old Bearer-token-based middleware (lines 89-113) with session+role:

```rust
use crate::api::auth::extract_session_user;
use axum::http::header::COOKIE;

async fn admin_auth(
    State(state): State<Arc<AppState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract session from cookie
    let headers = request.headers().clone();
    let user = extract_session_user(&state, &headers)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Check role — must be admin or super_admin
    if user.role == "user" {
        return Err(StatusCode::FORBIDDEN);
    }

    // Inject user into request extensions for downstream handlers
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}
```

- [ ] **Step 2: Add `CurrentUser` extractor type**

In `src/api/admin/mod.rs`, add a helper extractor:

```rust
pub mod proxies;
pub mod settings;
pub mod users;

use crate::db::User;
use axum::extract::Extension;

/// Type alias for extracting the current admin user from request extensions.
/// Set by the admin_auth middleware.
pub type CurrentUser = Extension<User>;
```

- [ ] **Step 3: Add default super_admin creation in `src/main.rs`**

After `seed_settings_to_db` (around line 53), add:

```rust
// Ensure at least one super_admin exists
match db.count_users_by_role("super_admin") {
    Ok(count) if count == 0 => {
        use argon2::{Argon2, PasswordHasher};
        use argon2::password_hash::{SaltString, rand_core::OsRng};

        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(b"admin", &salt)
            .expect("Failed to hash default password")
            .to_string();

        match db.create_password_user("admin", &hash, 0) {
            Ok(user) => {
                db.update_user_role(&user.id, "super_admin").ok();
                tracing::warn!("=== Created default super_admin: admin/admin — CHANGE THIS IMMEDIATELY ===");
            }
            Err(e) => tracing::error!("Failed to create default super_admin: {e}"),
        }
    }
    Ok(_) => {} // At least one super_admin exists
    Err(e) => tracing::error!("Failed to check super_admin count: {e}"),
}
```

- [ ] **Step 4: Add role change endpoint to `src/api/admin/users.rs`**

```rust
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
```

- [ ] **Step 5: Add permission checks to `delete_user` in `users.rs`**

Replace the current simple delete handler with permission-checked version:

```rust
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
```

- [ ] **Step 6: Register new route in `src/api/mod.rs`**

Add the role change route to admin_routes:

```rust
.route("/api/admin/users/:id/role", put(admin::users::change_user_role))
```

- [ ] **Step 7: Remove admin_password from admin.html login flow**

The admin.html currently has a password prompt overlay. The new flow:
- On page load, call `/api/auth/me`
- If not logged in → redirect to `/`
- If role is `"user"` → show "权限不足" message + redirect link
- If role is `"admin"` or `"super_admin"` → show the admin dashboard
- Store current user info for permission-aware UI rendering

This will be done fully in T07, but ensure the backend is ready.

- [ ] **Step 8: Compile check + commit**

Run: `cargo build 2>&1 | head -30`
Expected: Compiles successfully.

```bash
git add -A
git commit -m "feat(v0.35): RBAC auth middleware + default super_admin + role management APIs"
```
