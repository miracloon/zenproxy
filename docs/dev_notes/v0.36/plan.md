# v0.36 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple port allocation from validation status by introducing an enabled/disabled-driven port model, add port memory for disabled proxies, support validation of disabled proxies via temporary ports, and fix multiple UI issues.

**Architecture:** Core refactor centers on `sync_proxy_bindings` (remove SyncMode, enabled-only logic with port memory reservation) and `validate_all` (two-phase: enabled direct + disabled temp ports). New `validate_disabled` function shares phase-2 logic. Subscription adds default to disabled. Frontend gets batch operation buttons, local port display, modal components, and user password change.

**Tech Stack:** Rust/Axum (backend), SQLite (DB), vanilla HTML/CSS/JS (frontend), argon2 (password hashing)

---

## File Structure

### Modified Files (by task)

| File | Tasks | Changes |
|------|-------|---------|
| `src/db.rs` | T01 | Add `disabled_at` column migration, `port_retention_hours` setting, DB methods for remembered ports query/cleanup |
| `src/api/subscription.rs` | T02, T03 | Remove `SyncMode`, rewrite `sync_proxy_bindings`, update `add_subscription` (default disabled + notice), update `refresh_subscription_core` |
| `src/pool/validator.rs` | T02 | Rewrite `validate_all` (two-phase), new `validate_disabled_proxies` shared function |
| `src/quality/checker.rs` | T02 | Rewrite `check_all` (enabled+valid only, no sync calls) |
| `src/singbox/process.rs` | T02 | No interface change — existing `create_binding`/`create_binding_on_port`/`remove_binding` reused |
| `src/api/admin/proxies.rs` | T03, T02 | Add batch APIs (enable-valid, disable-invalid, validate-disabled), add try_lock to toggle handler |
| `src/main.rs` | T03 | Add port memory cleanup timer in `start_background_tasks` |
| `src/api/auth.rs` | T04 | Fix `/api/auth/me` response (add `auth_source`), new `PUT /api/auth/password` handler |
| `src/api/admin/users.rs` | T04 | Add `super_admin` role check to username/password handlers |
| `src/api/mod.rs` | T04, T05 | Add new routes (`/api/auth/password`, favicon) |
| `src/web/admin.html` | T06 | Local port column, batch buttons (enable-valid, disable-invalid, validate-disabled), modal component, toggle disabled during validation, checkbox fix, trust level fix |
| `src/web/user.html` | T06 | Local port column, user dropdown menu, change password modal, `auth_source` rendering, favicon link |
| `src/web/docs.html` | T05 | Favicon link in head |

---

## Task Dependency Graph

```
T01 (DB Schema) ──→ T02 (Core Engine Refactor) ──→ T03 (Batch APIs + Cleanup Timer)
                                                         │
T04 (Auth + Permissions) ────────────────────────────────┤
                                                         │
T05 (Favicon + UI Fixes) ───────────────────────────────┤
                                                         ▼
                                                    T06 (Frontend)
```

**Critical path:** T01 → T02 → T03 → T06

**Parallelizable after T02:** T03, T04, T05 are mutually independent (all feed into T06).

---

## Task Summary

| Task | Name | Scope | Risk |
|------|------|-------|------|
| T01 | DB Schema + Port Memory Foundation | `disabled_at` migration, `port_retention_hours` setting, remembered ports DB methods | Low |
| T02 | Core Engine Refactor | Remove `SyncMode`, rewrite `sync_proxy_bindings` (Method A reservation), rewrite `validate_all` (two-phase), new `validate_disabled_proxies`, rewrite `check_all`, update `add_subscription` (default disabled), toggle try_lock | **High** |
| T03 | Batch APIs + Cleanup Timer | 3 batch endpoints, port memory cleanup timer | Low |
| T04 | Auth + Permissions | Fix `/api/auth/me`, `PUT /api/auth/password`, super_admin permission checks | Low |
| T05 | Favicon + Minor UI Fixes | Favicon routes, checkbox fix, trust level fix, HTML head links | Low |
| T06 | Frontend Updates | admin.html + user.html: local port column, batch buttons, modal component, user menu, change password, validation lock UI | Medium |

---

## Verification Strategy

This project has no automated test suite. Verification is done via:
1. `cargo build` — compilation check after each backend task
2. Manual API testing with `curl` commands (specified per task)
3. Visual inspection of frontend changes in browser
4. Docker build smoke test at the end (optional)

**T02 checkpoint:** After completing T02, perform end-to-end manual verification:
- Verify `sync_proxy_bindings` only allocates ports for enabled proxies
- Verify `validate_all` validates both enabled (phase 1) and disabled (phase 2) proxies
- Verify disabled proxy temp ports are not persisted to DB
- Verify port memory reservation prevents port conflicts
- Verify `check_all` only quality-checks enabled+valid proxies

---

## Detailed Task Descriptions

### Task 01: DB Schema + Port Memory Foundation

**Files:**
- Modify: `src/db.rs`

**What to do:**

- [ ] **Step 1: Add `disabled_at` column migration**

  In `run_migrations()`, add migration to check and add `disabled_at` column to `proxies` table:
  ```sql
  ALTER TABLE proxies ADD COLUMN disabled_at TEXT;
  ```
  Pattern: follow existing `is_disabled` migration (L197-206). Also add `disabled_at` to `ProxyRow` struct and all SELECT/INSERT queries that touch proxies.

- [ ] **Step 2: Add `port_retention_hours` default setting**

  In `seed_settings_to_db` (in `src/config.rs`), add `port_retention_hours` with default value `"24"` if not already set.

- [ ] **Step 3: Add DB helper methods**

  Add to `Database` impl:
  ```rust
  /// Get ports of disabled proxies still within retention period.
  /// Returns Vec<u16> of local_port values to reserve.
  pub fn get_remembered_ports(&self) -> Result<Vec<u16>>

  /// Clear local_port for disabled proxies past retention.
  pub fn clear_expired_port_memory(&self) -> Result<usize>

  /// Set disabled_at timestamp when disabling a proxy.
  pub fn set_proxy_disabled_at(&self, id: &str, disabled_at: Option<&str>) -> Result<()>
  ```

  `get_remembered_ports` query:
  ```sql
  SELECT local_port FROM proxies
  WHERE is_disabled = 1
    AND local_port IS NOT NULL
    AND disabled_at IS NOT NULL
    AND (julianday('now') - julianday(disabled_at)) * 24 <= ?
  ```
  The `?` parameter comes from `get_setting("port_retention_hours")`.

- [ ] **Step 4: Update toggle_proxy in proxies.rs to set disabled_at**

  When toggling a proxy to disabled, call `set_proxy_disabled_at(id, Some(now))`. When enabling, call `set_proxy_disabled_at(id, None)`.

- [ ] **Step 5: Compile and verify**

  Run: `cargo build`

- [ ] **Step 6: Commit**

  ```
  git add -A && git commit -m "feat(db): add disabled_at column, port memory helpers, port_retention_hours setting"
  ```

---

### Task 02: Core Engine Refactor

**Files:**
- Modify: `src/api/subscription.rs`
- Modify: `src/pool/validator.rs`
- Modify: `src/quality/checker.rs`
- Modify: `src/api/admin/proxies.rs`

**This is the highest-risk task.** All changes must be made atomically (single commit) because removing `SyncMode` breaks compilation in all dependent files.

**What to do:**

- [ ] **Step 1: Remove `SyncMode` and rewrite `sync_proxy_bindings`**

  In `src/api/subscription.rs`:
  1. Delete `SyncMode` enum (L55-60)
  2. Change `sync_proxy_bindings` signature to `pub async fn sync_proxy_bindings(state: &Arc<AppState>)` (no mode param)
  3. New logic:
     ```
     1. Get all proxies from pool
     2. Get remembered ports from DB (get_remembered_ports)
     3. Lock singbox
     4. Pre-occupy remembered ports in PortPool (allocate_specific, ignore errors)
     5. Separate proxies into: enabled (need ports) vs disabled (skip)
     6. For enabled proxies, up to max_proxies:
        - If has local_port and still in current_ports → keep
        - If has DB remembered port → create_binding_on_port (restore)
        - Otherwise → create_binding (new allocation)
     7. Release pre-occupied ports that weren't actually used
     8. Remove bindings for proxies no longer selected
     9. Update pool and DB with assignments
     ```

- [ ] **Step 2: Update all `sync_proxy_bindings` call sites**

  Remove the `SyncMode` argument from every call:
  - `src/api/subscription.rs`: `delete_subscription` (L196)
  - Remove calls from `validator.rs` and `checker.rs` (handled in steps 3-4)

- [ ] **Step 3: Rewrite `validate_all` in `src/pool/validator.rs`**

  New two-phase logic:
  ```rust
  pub async fn validate_all(state: Arc<AppState>) -> Result<(), String> {
      let _lock = state.validation_lock.lock().await;

      // Phase 1: validate enabled proxies with existing ports
      let enabled: Vec<_> = state.pool.get_all().into_iter()
          .filter(|p| !p.is_disabled && p.local_port.is_some())
          .collect();
      if !enabled.is_empty() {
          validate_batch(&enabled, &validation_url, timeout, concurrency, &state).await;
      }

      // Phase 2: validate disabled proxies with temp ports
      validate_disabled_proxies(&state, &validation_url, timeout, concurrency).await;

      // Cleanup high-error proxies (keep existing logic)
      // ...

      Ok(())
  }
  ```

- [ ] **Step 4: Implement `validate_disabled_proxies` shared function**

  New function in `src/pool/validator.rs`:
  ```rust
  pub async fn validate_disabled_proxies(
      state: &AppState,
      validation_url: &str,
      timeout: Duration,
      concurrency: usize,
  ) {
      let batch_size = /* read from settings */;
      let disabled: Vec<_> = state.pool.get_all().into_iter()
          .filter(|p| p.is_disabled)
          .collect();
      
      for batch in disabled.chunks(batch_size) {
          // Create temp bindings
          let mut temp_assignments = Vec::new();
          {
              let mut mgr = state.singbox.lock().await;
              for proxy in batch {
                  // Check if proxy has remembered port in DB
                  let result = if let Some(remembered) = /* DB lookup */ {
                      mgr.create_binding_on_port(&proxy.id, remembered, &proxy.singbox_outbound).await
                  } else {
                      mgr.create_binding(&proxy.id, &proxy.singbox_outbound).await
                  };
                  if let Ok(port) = result {
                      temp_assignments.push((proxy.clone(), port));
                  }
              }
          }

          // Validate through temp ports (set local_port in pool temporarily)
          for (proxy, port) in &temp_assignments {
              state.pool.set_local_port(&proxy.id, *port);
          }
          let to_validate: Vec<_> = temp_assignments.iter()
              .map(|(p, port)| { /* create PoolProxy with temp local_port */ })
              .collect();
          validate_batch(&to_validate, validation_url, timeout, concurrency, state).await;

          // Cleanup: remove temp bindings, restore pool state
          {
              let mut mgr = state.singbox.lock().await;
              for (proxy, port) in &temp_assignments {
                  mgr.remove_binding(&proxy.id, *port).await.ok();
                  state.pool.clear_local_port(&proxy.id);
                  // DO NOT write to DB — temp ports don't create memory
              }
          }
      }
  }
  ```

- [ ] **Step 5: Add standalone `validate_disabled_only` entry point**

  ```rust
  pub async fn validate_disabled_only(state: Arc<AppState>) -> Result<(), String> {
      let _lock = state.validation_lock.lock().await;
      let validation_url = /* read from settings */;
      let timeout = /* ... */;
      let concurrency = /* ... */;
      validate_disabled_proxies(&state, &validation_url, timeout, concurrency).await;
      Ok(())
  }
  ```

- [ ] **Step 6: Rewrite `check_all` in `src/quality/checker.rs`**

  Remove all `sync_proxy_bindings` calls. Only quality-check enabled + valid + has-port proxies:
  ```rust
  let candidates: Vec<_> = state.pool.get_all().into_iter()
      .filter(|p| !p.is_disabled && p.status == ProxyStatus::Valid && p.local_port.is_some())
      .filter(|p| needs_quality_check(p, &now))
      .collect();
  ```
  No temporary port allocation. No `sync_proxy_bindings` calls.

- [ ] **Step 7: Update `add_subscription` default state**

  In `src/api/subscription.rs`, `add_subscription` function:
  - Change `is_disabled: false` → `is_disabled: true` for all new proxies
  - Change `status: ProxyStatus::Untested` → `ProxyStatus::Disabled`
  - Add `notice` field to response JSON
  - Keep the background `validate_all` spawn (it will validate disabled proxies via phase 2)

- [ ] **Step 8: Add try_lock to toggle handler**

  In `src/api/admin/proxies.rs`, `toggle_proxy` handler:
  ```rust
  if state.validation_lock.try_lock().is_err() {
      return Err(AppError::Conflict("验证进行中，请稍后操作".into()));
  }
  ```

- [ ] **Step 9: Compile and verify**

  Run: `cargo build`
  
  Expected: clean compilation with no `SyncMode` references remaining.

- [ ] **Step 10: Manual verification checkpoint**

  Test critical paths:
  1. Start server, verify initial bindings work
  2. Toggle proxy → verify port allocated/released
  3. Add subscription → verify proxies default disabled
  4. Trigger validate_all → verify both phases execute
  5. Verify disabled proxy temp ports not in DB

- [ ] **Step 11: Commit**

  ```
  git add -A && git commit -m "refactor: decouple port allocation from validation, two-phase validate_all, remove SyncMode"
  ```

---

### Task 03: Batch APIs + Cleanup Timer

**Files:**
- Modify: `src/api/admin/proxies.rs`
- Modify: `src/api/mod.rs`
- Modify: `src/main.rs`

**What to do:**

- [ ] **Step 1: Add enable-valid endpoint**

  `POST /api/admin/proxies/enable-valid`:
  ```rust
  // Collect all Valid + disabled proxies
  // Set is_disabled = false, disabled_at = None for each
  // Call sync_proxy_bindings to assign ports
  // Return count of enabled proxies
  ```

- [ ] **Step 2: Add disable-invalid endpoint**

  `POST /api/admin/proxies/disable-invalid`:
  ```rust
  // Collect all Invalid + enabled proxies
  // Set is_disabled = true, disabled_at = now for each
  // Call sync_proxy_bindings to release ports
  // Return count of disabled proxies
  ```

- [ ] **Step 3: Add validate-disabled endpoint**

  `POST /api/admin/proxies/validate-disabled`:
  ```rust
  // Spawn validate_disabled_only in background
  // Return immediately with status message
  ```

- [ ] **Step 4: Register routes**

  In `src/api/mod.rs`, add the three new routes under admin proxy management.

- [ ] **Step 5: Add port memory cleanup timer**

  In `src/main.rs` `start_background_tasks`, add hourly cleanup:
  ```rust
  tokio::spawn(async move {
      let interval = std::time::Duration::from_secs(3600); // 1 hour
      loop {
          tokio::time::sleep(interval).await;
          match state_clone.db.clear_expired_port_memory() {
              Ok(count) if count > 0 => tracing::info!("Cleared port memory for {count} proxies"),
              _ => {}
          }
      }
  });
  ```

- [ ] **Step 6: Compile, verify, commit**

  ```
  cargo build
  git add -A && git commit -m "feat: batch enable/disable/validate APIs, port memory cleanup timer"
  ```

---

### Task 04: Auth + Permissions

**Files:**
- Modify: `src/api/auth.rs`
- Modify: `src/api/admin/users.rs`
- Modify: `src/api/mod.rs`

**What to do:**

- [ ] **Step 1: Fix `/api/auth/me` response**

  Add `auth_source` to the JSON response (L217-226):
  ```rust
  Ok(Json(json!({
      "id": user.id,
      "username": user.username,
      // ... existing fields ...
      "auth_source": user.auth_source,  // ADD THIS
  })))
  ```

- [ ] **Step 2: Add `PUT /api/auth/password` handler**

  New handler in `src/api/auth.rs`:
  ```rust
  pub async fn change_password(
      State(state): State<Arc<AppState>>,
      headers: HeaderMap,
      Json(req): Json<ChangePasswordRequest>,
  ) -> Result<Json<serde_json::Value>, AppError> {
      let user = extract_session_user(&state, &headers).await?;
      if user.auth_source != "password" {
          return Err(AppError::BadRequest("OAuth users cannot change password"));
      }
      // Verify old password
      // Hash new password
      // Update DB
      // Return success
  }
  ```

- [ ] **Step 3: Add super_admin permission checks**

  In `src/api/admin/users.rs`, for `update_username` and `reset_password` handlers:
  ```rust
  let current_user = extract_session_user(&state, &headers).await?;
  if current_user.role != "super_admin" {
      return Err(AppError::Forbidden("Only super_admin can perform this action"));
  }
  ```

- [ ] **Step 4: Register route, compile, commit**

  Add `PUT /api/auth/password` route. Build and commit:
  ```
  cargo build
  git add -A && git commit -m "feat: fix auth/me, add user password change, super_admin permission checks"
  ```

---

### Task 05: Favicon + Minor UI Fixes

**Files:**
- Modify: `src/api/mod.rs`
- Modify: `src/web/admin.html`
- Modify: `src/web/user.html`
- Modify: `src/web/docs.html`

**What to do:**

- [ ] **Step 1: Add favicon routes**

  In `src/api/mod.rs`, add routes:
  ```rust
  .route("/favicon.ico", get(serve_favicon))
  .route("/icon.png", get(serve_icon))
  ```
  Handlers read from `data/favicon.ico` and `data/icon.png`, return with correct Content-Type, or 404.

- [ ] **Step 2: Add favicon links to HTML heads**

  Add to `<head>` of `admin.html`, `user.html`, `docs.html`:
  ```html
  <link rel="icon" href="/favicon.ico" type="image/x-icon">
  <link rel="icon" href="/icon.png" type="image/png">
  <link rel="apple-touch-icon" href="/icon.png">
  ```

- [ ] **Step 3: Fix checkbox UI**

  In `admin.html`, find the OAuth/registration checkbox patterns and fix:
  ```html
  <!-- Fix: move checkbox inside label, remove Unicode ☑ -->
  <label><input type="checkbox" id="set-linuxdo-enabled"> 启用 Linux.do OAuth 登录</label>
  ```

- [ ] **Step 4: Fix trust level range**

  Change `max="4"` → `max="3"`, update hint text from "0~4" to "0~3".

- [ ] **Step 5: Compile, verify, commit**

  ```
  cargo build
  git add -A && git commit -m "feat: favicon endpoint, checkbox fix, trust level range fix"
  ```

---

### Task 06: Frontend Updates

**Files:**
- Modify: `src/web/admin.html`
- Modify: `src/web/user.html`

**This task has no backend dependencies but requires all backend tasks (T01-T05) to be complete for full integration.**

**What to do:**

- [ ] **Step 1: Add local port column to admin proxy table**

  In the proxy table header and `renderProxyRow`, add "本地端口" column. Show `p.local_port` or `—` if null.

- [ ] **Step 2: Add batch operation buttons to admin**

  Add three buttons above proxy table:
  - "一键启用有效代理" → `POST /api/admin/proxies/enable-valid`
  - "一键禁用无效代理" → `POST /api/admin/proxies/disable-invalid`
  - "验证未启用代理" → `POST /api/admin/proxies/validate-disabled`
  
  Each button shows loading state and refreshes proxy list on completion.

- [ ] **Step 3: Add validation lock UI**

  Extend `isValidating` state to disable toggle buttons, batch buttons, and individual validate buttons during any validation operation. "验证全部" / "验证未启用代理" running → disable all proxy operation buttons.

- [ ] **Step 4: Build modal component**

  Create reusable modal (dark theme, rounded corners, consistent with existing UI):
  ```javascript
  function showModal(title, fields, onSubmit) { ... }
  function closeModal() { ... }
  ```
  Replace all `prompt()` calls with `showModal()`.

- [ ] **Step 5: Replace admin prompt() calls**

  - "改名" → modal with single input field
  - "重置密码" → modal with single password field
  - Hide both buttons for non-super_admin users

- [ ] **Step 6: Add local port column to user dashboard**

  In `user.html` proxy table, add "本地端口" column.

- [ ] **Step 7: Build user dropdown menu**

  Replace static username display with clickable dropdown:
  - Show username
  - Click → dropdown with "修改密码"（only for `auth_source === 'password'`）and "退出登录"

- [ ] **Step 8: Add change password modal**

  For password users: modal with old password, new password, confirm new password fields.
  Call `PUT /api/auth/password`. Show success/error toast.

- [ ] **Step 9: Fix auth_source rendering**

  Ensure `currentUser.auth_source` is available (from fixed `/api/auth/me`). Fix the default password warning banner condition.

- [ ] **Step 10: Visual verification**

  Test in browser:
  - Admin: proxy table with ports, batch buttons, modal dialogs, validation lock
  - User: proxy table with ports, dropdown menu, password change (for password users)
  - Both: favicon displays correctly

- [ ] **Step 11: Commit**

  ```
  git add -A && git commit -m "feat: frontend batch buttons, modal component, user password change, local port display"
  ```

---

## Post-Implementation

After all tasks complete, create `summary.md` per DEV_NOTES_WORKFLOW §7.
