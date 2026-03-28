# v0.37 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decouple proxy status from disabled state, add multi-select with batch operations, fix quality check safety net, and reorganize admin UI for scalable proxy management.

**Architecture:** Core change removes `ProxyStatus::Disabled` to make validation status and enabled status orthogonal. Frontend adds checkbox multi-select with cross-page persistence, four-zone action layout, and concurrent operation limits. New generic `POST /proxies/batch` API serves all selection-based operations. Quality checker gains an "all-probes-failed" safety net to auto-invalidate dead proxies.

**Tech Stack:** Rust/Axum (backend), SQLite (DB), vanilla HTML/CSS/JS (frontend)

---

## File Structure

### Modified Files

| File | Tasks | Changes |
|------|-------|---------|
| `src/pool/manager.rs` | T01 | Remove `ProxyStatus::Disabled`, modify `set_disabled()`, `load_from_db()`, `sort_weight()`, `set_status()` |
| `src/api/admin/proxies.rs` | T01, T02, T03 | Remove is_disabled checks from validate/quality handlers, add port memory to single ops, new batch API, new validate-invalid API |
| `src/quality/checker.rs` | T01, T02 | Expand `check_all` to include disabled+valid, add all-probes-failed safety net |
| `src/api/mod.rs` | T03 | Register new batch routes |
| `src/pool/validator.rs` | T03 | New `validate_invalid_only` function |
| `src/web/admin.html` | T04, T05, T06 | Status display, filter bar, action column, multi-select, batch operations, action zones, pagination config |

### No New Files Created

All changes are modifications to existing files.

---

## Task Dependency Graph

```
T01 (Status Model Decouple) ─→ T02 (Single Op Fixes) ─→ T03 (Batch API)
                                                              │
                                                              ▼
                                                         T04 (Frontend: Status + Actions)
                                                              │
                                                              ▼
                                                         T05 (Frontend: Multi-Select + Batch)
                                                              │
                                                              ▼
                                                         T06 (Frontend: Polish)
```

**Critical path:** T01 → T02 → T03 → T04 → T05 → T06

T04-T06 are sequential because they all modify the same file (`admin.html`).

---

## Task Summary

| Task | Name | Scope | Risk |
|------|------|-------|------|
| T01 | Status Model Decouple | Remove `ProxyStatus::Disabled`, fix `set_disabled`/`load_from_db`, adapt all references | **Medium** — wide touch surface |
| T02 | Single Operation Fixes + Safety Net | Port memory for single validate/quality, remove disable guards, quality all-probes-failed safety net | Medium |
| T03 | Batch API + Validate Invalid | Generic batch endpoint, validate-invalid-only function | Low |
| T04 | Frontend: Status Display + Actions | Dual badge, dual filter dropdowns, fixed action column, quality button disabled logic, concurrent limits | Medium |
| T05 | Frontend: Multi-Select + Batch UI | Checkbox column, select state management, action zones, batch operation calls | Medium |
| T06 | Frontend: Pagination + Polish | Page size selector, localStorage, minor polish | Low |

---

## Verification Strategy

No automated test suite. Verification via:
1. `cargo build` after each backend task
2. Manual API testing with `curl` (specified per task)
3. Visual browser testing for frontend tasks
4. Full integration walkthrough after T06

---

## Detailed Task Descriptions

### Task 01: Status Model Decouple

**Files:**
- Modify: `src/pool/manager.rs`

**What to do:**

- [ ] **Step 1: Remove `ProxyStatus::Disabled` from enum**

  In `src/pool/manager.rs` L8-13, change:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
  #[serde(rename_all = "lowercase")]
  pub enum ProxyStatus {
      Untested,
      Valid,
      Invalid,
      // Disabled removed
  }
  ```

- [ ] **Step 2: Update `sort_weight()`**

  L15-24, remove `Disabled => 3` branch:
  ```rust
  pub fn sort_weight(self) -> u8 {
      match self {
          ProxyStatus::Valid => 0,
          ProxyStatus::Untested => 1,
          ProxyStatus::Invalid => 2,
      }
  }
  ```

- [ ] **Step 3: Update `set_status()` match**

  L159-167, remove `Disabled` arm:
  ```rust
  pub fn set_status(&self, id: &str, status: ProxyStatus) {
      if let Some(mut proxy) = self.proxies.get_mut(id) {
          proxy.status = status;
          match status {
              ProxyStatus::Valid => proxy.error_count = 0,
              ProxyStatus::Invalid => proxy.error_count += 1,
              ProxyStatus::Untested => {}
          }
      }
  }
  ```

- [ ] **Step 4: Fix `set_disabled()` — do NOT change status**

  L202-212, replace with:
  ```rust
  pub fn set_disabled(&self, id: &str, disabled: bool) {
      if let Some(mut proxy) = self.proxies.get_mut(id) {
          proxy.is_disabled = disabled;
          // Do NOT modify proxy.status — preserve validation state
      }
  }
  ```

- [ ] **Step 5: Fix `load_from_db()` — do NOT use is_disabled for status**

  L106-115, replace status derivation:
  ```rust
  let status = if row.is_valid {
      ProxyStatus::Valid
  } else if row.last_validated.is_some() {
      ProxyStatus::Invalid
  } else {
      ProxyStatus::Untested
  };
  // is_disabled is loaded separately into proxy.is_disabled, does NOT affect status
  ```

- [ ] **Step 6: Fix `batch_enable_valid` in `src/api/admin/proxies.rs`**

  L304-315 currently filters for `ProxyStatus::Disabled` — replace with `is_disabled` field check only:
  ```rust
  // Remove this block that checks for ProxyStatus::Disabled
  // Replace with: just check is_disabled field
  let all_targets: Vec<String> = state.pool.get_all()
      .iter()
      .filter(|p| p.is_disabled && p.status == ProxyStatus::Valid)
      .map(|p| p.id.clone())
      .collect();
  ```

- [ ] **Step 7: Compile and verify**

  Run: `cargo build`

  Expected: clean compilation. Search for any remaining `ProxyStatus::Disabled` references — there should be none.

  ```bash
  grep -rn "ProxyStatus::Disabled\|Disabled =>" src/
  ```
  Expected: 0 matches.

- [ ] **Step 8: Commit**

  ```
  git add -A && git commit -m "refactor: remove ProxyStatus::Disabled, decouple status from enabled state"
  ```

---

### Task 02: Single Operation Fixes + Safety Net

**Files:**
- Modify: `src/api/admin/proxies.rs`
- Modify: `src/quality/checker.rs`

**What to do:**

- [ ] **Step 1: Fix `validate_single_proxy` — remove disabled guard, add port memory**

  In `src/api/admin/proxies.rs` L147-211:

  1. Remove L154-156 (`if proxy.is_disabled` check)
  2. Replace L170-183 temp binding logic with port-memory-aware version:

  ```rust
  // Get or create binding (with port memory)
  let (local_port, temp_binding) = match state_clone.pool.get(&proxy_id) {
      Some(p) if p.local_port.is_some() => (p.local_port.unwrap(), false),
      Some(p) => {
          // Check DB for remembered port
          let remembered = state_clone.db.get_all_proxies().ok()
              .and_then(|rows| rows.into_iter()
                  .find(|r| r.id == proxy_id)
                  .and_then(|r| r.local_port.map(|port| port as u16)));

          let mut mgr = state_clone.singbox.lock().await;
          let result = if let Some(port) = remembered {
              mgr.create_binding_on_port(&proxy_id, port, &p.singbox_outbound).await
          } else {
              mgr.create_binding(&proxy_id, &p.singbox_outbound).await
          };
          match result {
              Ok(port) => (port, true),
              Err(e) => {
                  tracing::error!("Failed to create temp binding for {}: {e}", proxy_id);
                  return;
              }
          }
      }
      None => return,
  };
  ```

- [ ] **Step 2: Fix `quality_check_single_proxy` — remove disabled guard, add port memory**

  In `src/api/admin/proxies.rs` L239-287:

  1. Remove L246-248 (`if proxy.is_disabled` check)
  2. Keep L249-251 (`if proxy.status != Valid` check) — this is the intentional gate
  3. Replace L257-270 temp binding logic with same port-memory-aware pattern as Step 1

- [ ] **Step 3: Add all-probes-failed safety net to `check_batch`**

  In `src/quality/checker.rs` L134-186, in the `Ok(quality)` branch, add detection before saving:

  ```rust
  Ok(quality) => {
      // Safety net: if all probes failed, proxy is likely unreachable
      let all_failed = quality.ip_address.is_none()
          && !quality.google_accessible
          && !quality.chatgpt_accessible;

      if all_failed {
          state.pool.set_status(&proxy.id, crate::pool::manager::ProxyStatus::Invalid);
          state.db.update_proxy_validation(&proxy.id, false,
              Some("Quality check: all probes failed, proxy likely unreachable")).ok();
          tracing::warn!("All quality probes failed for {}, marking Invalid", proxy.name);
          return;
      }

      // ... existing quality save logic ...
  }
  ```

- [ ] **Step 4: Add same safety net to `check_single_proxy`**

  In `src/quality/checker.rs` L73-106, in the `Ok(quality)` match:

  ```rust
  Ok(quality) => {
      let all_failed = quality.ip_address.is_none()
          && !quality.google_accessible
          && !quality.chatgpt_accessible;

      if all_failed {
          state.pool.set_status(&proxy.id, crate::pool::manager::ProxyStatus::Invalid);
          state.db.update_proxy_validation(&proxy.id, false,
              Some("Quality check: all probes failed")).ok();
          tracing::warn!("Single quality check all probes failed for {}", proxy.id);
          return Err("All quality probes failed, proxy likely unreachable".into());
      }

      // ... existing save logic ...
  }
  ```

- [ ] **Step 5: Expand `check_all` scope to include disabled+valid**

  In `src/quality/checker.rs` L46-50, change filter:

  ```rust
  // Before: only enabled + valid + has-port
  // After: all valid (including disabled), will need temp ports for disabled
  let mut to_check: Vec<PoolProxy> = state
      .pool
      .get_all()
      .into_iter()
      .filter(|p| p.status == crate::pool::manager::ProxyStatus::Valid && p.local_port.is_some())
      .filter(|p| needs_quality_check(p, &now))
      .collect();
  ```

  Note: For disabled proxies without `local_port`, a separate batch with temp port allocation is needed. This mirrors `validate_disabled_proxies` pattern. Add after the existing check:

  ```rust
  // Also check disabled+valid proxies that need quality check (use temp ports)
  let disabled_valid: Vec<PoolProxy> = state
      .pool
      .get_all()
      .into_iter()
      .filter(|p| p.is_disabled && p.status == crate::pool::manager::ProxyStatus::Valid && p.local_port.is_none())
      .filter(|p| needs_quality_check(p, &now))
      .collect();

  if !disabled_valid.is_empty() {
      let batch_size = 10; // smaller batches for quality (rate-limited)
      for batch in disabled_valid.chunks(batch_size) {
          // Create temp bindings (with port memory — same pattern as validate_disabled_proxies)
          // ... temp binding setup ...
          total_checked += check_batch(&to_check_with_ports, &state, &rate_limiter).await;
          // ... cleanup temp bindings ...
      }
  }
  ```

- [ ] **Step 6: Compile and verify**

  ```bash
  cargo build
  ```

- [ ] **Step 7: Commit**

  ```
  git add -A && git commit -m "fix: port memory for single ops, quality safety net, expand check_all scope"
  ```

---

### Task 03: Batch API + Validate Invalid

**Files:**
- Modify: `src/api/admin/proxies.rs`
- Modify: `src/pool/validator.rs`
- Modify: `src/api/mod.rs` (or `src/api/admin/mod.rs`)

**What to do:**

- [ ] **Step 1: Add `validate_invalid_only` to validator**

  In `src/pool/validator.rs`, add new function:

  ```rust
  /// Validate only proxies with status == Invalid, using temp ports for disabled ones.
  pub async fn validate_invalid_only(state: Arc<AppState>) -> Result<(), String> {
      let _lock = state.validation_lock.lock().await;

      let concurrency = /* read from settings */;
      let timeout_duration = /* read from settings */;
      let validation_url = /* read from settings */;

      // Enabled invalid proxies — validate with existing ports
      let enabled_invalid: Vec<_> = state.pool.get_all().into_iter()
          .filter(|p| !p.is_disabled && p.status == ProxyStatus::Invalid && p.local_port.is_some())
          .collect();
      if !enabled_invalid.is_empty() {
          validate_batch(&enabled_invalid, &validation_url, timeout_duration, concurrency, &state).await;
      }

      // Disabled invalid proxies — validate with temp ports
      let disabled_invalid: Vec<_> = state.pool.get_all().into_iter()
          .filter(|p| p.is_disabled && p.status == ProxyStatus::Invalid)
          .collect();
      if !disabled_invalid.is_empty() {
          // Reuse validate_disabled_proxies pattern with filtered list
          // ... temp port allocation, validate, cleanup ...
      }

      Ok(())
  }
  ```

- [ ] **Step 2: Add `validate-invalid` API endpoint**

  In `src/api/admin/proxies.rs`:
  ```rust
  pub async fn batch_validate_invalid(
      State(state): State<Arc<AppState>>,
  ) -> Result<Json<serde_json::Value>, AppError> {
      let state_clone = state.clone();
      tokio::spawn(async move {
          if let Err(e) = crate::pool::validator::validate_invalid_only(state_clone).await {
              tracing::error!("Batch validate-invalid failed: {e}");
          }
      });
      Ok(Json(json!({ "message": "已开始验证无效代理" })))
  }
  ```

- [ ] **Step 3: Add generic batch endpoint**

  In `src/api/admin/proxies.rs`:

  ```rust
  #[derive(serde::Deserialize)]
  pub struct BatchRequest {
      action: String,
      ids: Vec<String>,
  }

  pub async fn batch_proxy_action(
      State(state): State<Arc<AppState>>,
      Json(req): Json<BatchRequest>,
  ) -> Result<Json<serde_json::Value>, AppError> {
      if state.validation_lock.try_lock().is_err()
          && matches!(req.action.as_str(), "enable" | "disable")
      {
          return Err(AppError::Conflict("验证进行中，请稍后操作".into()));
      }

      let proxies: Vec<_> = req.ids.iter()
          .filter_map(|id| state.pool.get(id))
          .collect();
      let total = proxies.len();

      match req.action.as_str() {
          "enable" => {
              let targets: Vec<_> = proxies.iter().filter(|p| p.is_disabled).collect();
              let processed = targets.len();
              for p in &targets {
                  state.pool.set_disabled(&p.id, false);
                  state.db.set_proxy_disabled(&p.id, false).ok();
              }
              if processed > 0 {
                  let s2 = state.clone();
                  tokio::spawn(async move { crate::api::subscription::sync_proxy_bindings(&s2).await; });
              }
              Ok(Json(json!({
                  "action": "enable", "total": total, "processed": processed,
                  "skipped": total - processed,
                  "message": format!("已启用 {} 个，跳过 {} 个(已启用)", processed, total - processed)
              })))
          }
          "disable" => {
              let targets: Vec<_> = proxies.iter().filter(|p| !p.is_disabled).collect();
              let processed = targets.len();
              for p in &targets {
                  state.pool.set_disabled(&p.id, true);
                  state.db.set_proxy_disabled(&p.id, true).ok();
                  if let Some(port) = p.local_port {
                      let mut mgr = state.singbox.lock().await;
                      mgr.remove_binding(&p.id, port).await.ok();
                      state.pool.clear_local_port(&p.id);
                  }
              }
              Ok(Json(json!({
                  "action": "disable", "total": total, "processed": processed,
                  "skipped": total - processed,
                  "message": format!("已禁用 {} 个，跳过 {} 个(已禁用)", processed, total - processed)
              })))
          }
          "validate" => {
              // Spawn background validation for all specified IDs
              let ids = req.ids.clone();
              let state2 = state.clone();
              tokio::spawn(async move {
                  // For each proxy, run single validation (reuse validate_single logic)
                  for id in &ids {
                      // ... single validation with port memory ...
                  }
              });
              Ok(Json(json!({
                  "action": "validate", "total": total, "processed": total,
                  "skipped": 0,
                  "message": format!("已启动 {} 个代理的连通测试", total)
              })))
          }
          "quality" => {
              let targets: Vec<_> = proxies.iter()
                  .filter(|p| p.status == crate::pool::manager::ProxyStatus::Valid)
                  .collect();
              let processed = targets.len();
              let skipped = total - processed;
              // Spawn background quality check
              let ids: Vec<String> = targets.iter().map(|p| p.id.clone()).collect();
              let state2 = state.clone();
              tokio::spawn(async move {
                  for id in &ids {
                      // ... single quality check with port memory ...
                  }
              });
              Ok(Json(json!({
                  "action": "quality", "total": total, "processed": processed, "skipped": skipped,
                  "message": format!("已启动 {} 个代理的质检，跳过 {} 个(非有效)", processed, skipped)
              })))
          }
          "delete" => {
              for p in &proxies {
                  state.pool.remove(&p.id);
                  state.db.delete_proxy(&p.id).ok();
                  if let Some(port) = p.local_port {
                      let mut mgr = state.singbox.lock().await;
                      mgr.remove_binding(&p.id, port).await.ok();
                  }
              }
              Ok(Json(json!({
                  "action": "delete", "total": total, "processed": total,
                  "skipped": 0,
                  "message": format!("已删除 {} 个代理", total)
              })))
          }
          _ => Err(AppError::BadRequest("Unknown batch action".into())),
      }
  }
  ```

- [ ] **Step 4: Register routes**

  In route registration:
  ```rust
  .route("/api/admin/proxies/batch", post(batch_proxy_action))
  .route("/api/admin/proxies/validate-invalid", post(batch_validate_invalid))
  ```

- [ ] **Step 5: Compile and verify**

  ```bash
  cargo build
  ```

- [ ] **Step 6: Commit**

  ```
  git add -A && git commit -m "feat: generic batch API, validate-invalid endpoint"
  ```

---

### Task 04: Frontend — Status Display + Action Column

**Files:**
- Modify: `src/web/admin.html`

**What to do:**

- [ ] **Step 1: Add dual filter dropdowns**

  Replace the single `f-status` select (L229-235) with two:

  ```html
  <select id="f-validity" onchange="currentPage=1;renderProxies()">
      <option value="">全部验证状态</option>
      <option value="valid">有效</option>
      <option value="invalid">无效</option>
      <option value="untested">待测试</option>
  </select>
  <select id="f-enabled" onchange="currentPage=1;renderProxies()">
      <option value="">全部启用状态</option>
      <option value="enabled">已启用</option>
      <option value="disabled">已禁用</option>
  </select>
  ```

- [ ] **Step 2: Update `renderProxies()` filter logic**

  Replace L504-507 filter:
  ```javascript
  const validity = document.getElementById('f-validity').value;
  const enabled = document.getElementById('f-enabled').value;

  let filtered = allProxies.filter(p => {
      if(search && !p.name.toLowerCase().includes(search) && !p.server.toLowerCase().includes(search)) return false;
      if(validity && p.status !== validity) return false;
      if(enabled === 'enabled' && p.is_disabled) return false;
      if(enabled === 'disabled' && !p.is_disabled) return false;
      if(type_ && p.type !== type_) return false;
      // quality filters unchanged
      return true;
  });
  ```

- [ ] **Step 3: Update status badge to dual badge**

  Replace L556-557:
  ```javascript
  const validityBadge = p.status === 'valid'
      ? '<span class="badge badge-valid">有效</span>'
      : p.status === 'untested'
          ? '<span class="badge badge-untested">待测试</span>'
          : '<span class="badge badge-invalid">无效</span>';
  const disabledBadge = p.is_disabled ? ' <span class="badge badge-disabled">已禁用</span>' : '';
  const statusBadge = validityBadge + disabledBadge;
  ```

- [ ] **Step 4: Fix action column — 4 fixed buttons**

  Replace L559-578 button rendering:
  ```javascript
  const isChecking = pendingActions[p.id];
  const toggleBtn = p.is_disabled
      ? `<button class="btn-xs btn-success" onclick="toggleProxy('${esc(p.id)}')">启用</button>`
      : `<button class="btn-xs btn-warning" onclick="toggleProxy('${esc(p.id)}')">禁用</button>`;
  const connectBtn = isChecking === 'validate'
      ? `<button class="btn-xs btn-info" disabled><span class="spinner-xs"></span>检测中</button>`
      : `<button class="btn-xs btn-info" onclick="validateSingle('${esc(p.id)}')" ${!canStartValidate()?'disabled':''}">连通测试</button>`;
  const qualityBtn = isChecking === 'quality'
      ? `<button class="btn-xs" disabled><span class="spinner-xs"></span>质检中</button>`
      : `<button class="btn-xs" onclick="qualitySingle('${esc(p.id)}')" ${p.status !== 'valid' || !canStartQuality()?'disabled':''}>质量检测</button>`;
  const deleteBtn = `<button class="btn-xs btn-danger" onclick="deleteProxy('${p.id}')">删除</button>`;
  ```

  Add CSS for fixed width:
  ```css
  .actions-cell { display:flex; gap:4px; min-width:300px; }
  .actions-cell .btn-xs { min-width:64px; text-align:center; }
  ```

- [ ] **Step 5: Add concurrent limit functions**

  ```javascript
  const MAX_CONCURRENT_QUALITY = 3;
  const MAX_CONCURRENT_VALIDATE = 5;

  function canStartQuality() {
      return Object.values(pendingActions).filter(v => v === 'quality').length < MAX_CONCURRENT_QUALITY;
  }
  function canStartValidate() {
      return Object.values(pendingActions).filter(v => v === 'validate').length < MAX_CONCURRENT_VALIDATE;
  }
  ```

  Update `validateSingle()` and `qualitySingle()` to check limits before executing:
  ```javascript
  async function validateSingle(id) {
      if(!canStartValidate()) { toast('连通测试并发已达上限，请等待'); return; }
      // ... existing logic
  }
  async function qualitySingle(id) {
      if(!canStartQuality()) { toast('质检并发已达上限，请等待'); return; }
      // ... existing logic
  }
  ```

- [ ] **Step 6: Update sort logic**

  L521 sort — replace disabled sort with `is_disabled` field:
  ```javascript
  case 'is_valid':
      va = p.status === 'valid' ? 0 : p.status === 'untested' ? 1 : 2;
      vb = b.status === 'valid' ? 0 : b.status === 'untested' ? 1 : 2;
      // Secondary sort by disabled
      if(va === vb) { va = p.is_disabled ? 1 : 0; vb = b.is_disabled ? 1 : 0; }
      break;
  ```

- [ ] **Step 7: Compile and verify**

  ```bash
  cargo build
  ```

- [ ] **Step 8: Commit**

  ```
  git add -A && git commit -m "feat(admin): dual status badges, dual filter dropdowns, fixed action column, concurrent limits"
  ```

---

### Task 05: Frontend — Multi-Select + Batch Operations

**Files:**
- Modify: `src/web/admin.html`

**What to do:**

- [ ] **Step 1: Add selection state management**

  ```javascript
  let selectedIds = new Set();

  function toggleSelect(id) {
      if(selectedIds.has(id)) selectedIds.delete(id);
      else selectedIds.add(id);
      updateSelectionUI();
      renderProxies();
  }

  function selectAll() {
      // Select all filtered results (cross-page)
      getFilteredProxies().forEach(p => selectedIds.add(p.id));
      updateSelectionUI();
      renderProxies();
  }

  function selectPage() {
      // Select only current page
      getCurrentPageProxies().forEach(p => selectedIds.add(p.id));
      updateSelectionUI();
      renderProxies();
  }

  function clearSelection() {
      selectedIds.clear();
      updateSelectionUI();
      renderProxies();
  }

  function getFilteredProxies() { /* extract filter logic from renderProxies into reusable fn */ }
  function getCurrentPageProxies() { /* filtered + paginated subset */ }

  function updateSelectionUI() {
      const count = selectedIds.size;
      document.getElementById('selection-count').textContent = count;
      document.querySelectorAll('.selection-action').forEach(btn => {
          btn.disabled = count === 0;
      });
  }
  ```

- [ ] **Step 2: Add checkbox column to table**

  In table header (L253):
  ```html
  <th style="width:32px"><input type="checkbox" id="select-all-page" onchange="selectPage()"></th>
  ```

  In row rendering:
  ```javascript
  `<td><input type="checkbox" ${selectedIds.has(p.id)?'checked':''} onchange="toggleSelect('${esc(p.id)}')"></td>`
  ```

- [ ] **Step 3: Add selection action zone (Zone ①)**

  Replace the current "操作" section (L157-168) with four-zone layout:

  ```html
  <!-- Zone 1: Selection Actions -->
  <div class="section" style="margin-bottom:12px">
      <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:8px">
          <button class="btn btn-sm" onclick="selectAll()">全选 (<span id="filtered-count">0</span>条)</button>
          <button class="btn btn-sm" onclick="selectPage()">选本页</button>
          <button class="btn btn-sm" onclick="clearSelection()" style="color:var(--text-dim)">取消选择</button>
          <span style="color:var(--text-dim);font-size:13px">已选 <strong id="selection-count">0</strong> 个</span>
      </div>
      <div class="btn-group">
          <button class="btn btn-success btn-sm selection-action" disabled onclick="batchSelected('enable')">启用选中</button>
          <button class="btn btn-warn btn-sm selection-action" disabled onclick="batchSelected('disable')">禁用选中</button>
          <button class="btn btn-primary btn-sm selection-action" disabled onclick="batchSelected('validate')">验证选中</button>
          <button class="btn btn-sm selection-action" disabled onclick="batchSelected('quality')">质检选中</button>
          <button class="btn btn-danger btn-sm selection-action" disabled onclick="batchSelected('delete')">删除选中</button>
      </div>
  </div>

  <!-- Zone 2: Validate & Quality -->
  <div class="section" style="margin-bottom:12px">
      <div class="section-title" style="font-size:13px;margin-bottom:8px">验证与质检</div>
      <div class="btn-group">
          <button class="btn btn-primary btn-sm" onclick="triggerValidation()">验证全部</button>
          <button class="btn btn-primary btn-sm" onclick="batchValidateDisabled()">验证未启用</button>
          <button class="btn btn-primary btn-sm" onclick="batchValidateInvalid()">验证无效</button>
          <button class="btn btn-sm" onclick="triggerQualityCheck()">质检全部</button>
      </div>
  </div>

  <div class="grid-2">
      <!-- Zone 3: Quick Actions -->
      <div class="section">
          <div class="section-title" style="font-size:13px;margin-bottom:8px">快捷操作</div>
          <div class="btn-group">
              <button class="btn btn-success btn-sm" onclick="batchEnableValid()">一键启用有效</button>
              <button class="btn btn-warn btn-sm" onclick="batchDisableInvalid()">一键禁用无效</button>
          </div>
      </div>
      <!-- Zone 4: Cleanup (dangerous) -->
      <div class="section">
          <div class="section-title" style="font-size:13px;margin-bottom:8px">清理</div>
          <div class="btn-group">
              <button class="btn btn-danger btn-sm" onclick="cleanupProxies()">清理无效</button>
              <button class="btn btn-danger btn-sm" onclick="cleanupUselessProxies()">清理三不通</button>
          </div>
      </div>
  </div>
  ```

- [ ] **Step 4: Implement batch action caller**

  ```javascript
  async function batchSelected(action) {
      const ids = Array.from(selectedIds);
      if(!ids.length) return;

      if(action === 'delete' && !confirm(`确定删除 ${ids.length} 个代理？`)) return;

      toast(`正在执行批量${actionName(action)}...`);
      const d = await api('/api/admin/proxies/batch', {
          method: 'POST',
          body: JSON.stringify({ action, ids })
      });
      if(d && d.message) toast(d.message);
      if(action === 'delete') selectedIds.clear();
      setTimeout(refresh, 1000);
  }

  function actionName(a) {
      return {enable:'启用',disable:'禁用',validate:'验证',quality:'质检',delete:'删除'}[a] || a;
  }

  async function batchValidateInvalid() {
      toast('正在验证无效代理...');
      await api('/api/admin/proxies/validate-invalid', { method: 'POST' });
  }
  ```

- [ ] **Step 5: Update filtered count display**

  In `renderProxies()`, after filtering:
  ```javascript
  document.getElementById('filtered-count').textContent = filtered.length;
  ```

- [ ] **Step 6: Commit**

  ```
  git add -A && git commit -m "feat(admin): multi-select checkboxes, batch operations, four-zone action layout"
  ```

---

### Task 06: Frontend — Pagination Config + Polish

**Files:**
- Modify: `src/web/admin.html`

**What to do:**

- [ ] **Step 1: Add page size selector**

  In filter bar, after the quality filter:
  ```html
  <select id="f-pagesize" onchange="changePageSize()" style="margin-left:auto">
      <option value="30">每页 30</option>
      <option value="50" selected>每页 50</option>
      <option value="100">每页 100</option>
  </select>
  ```

- [ ] **Step 2: Dynamic page size**

  Replace `const pageSize = 50;` (L392) with:
  ```javascript
  let pageSize = parseInt(localStorage.getItem('zenproxy_pagesize') || '50');

  function changePageSize() {
      pageSize = parseInt(document.getElementById('f-pagesize').value);
      localStorage.setItem('zenproxy_pagesize', pageSize);
      currentPage = 1;
      renderProxies();
  }
  ```

  In `init()`, restore:
  ```javascript
  document.getElementById('f-pagesize').value = pageSize;
  ```

- [ ] **Step 3: Remove `row-disabled` opacity class**

  L563 — remove the `row-disabled` class application. With dual-badge display, dimming the entire row is no longer needed (disabled state is explicitly shown via badge):
  ```javascript
  // Before: class="${p.is_disabled ? 'row-disabled' : ''}"
  // After: no special class
  return `<tr>
  ```

- [ ] **Step 4: Visual verification in browser**

  Test complete workflow:
  1. Dual filter dropdowns work independently
  2. Checkbox multi-select + select all/page
  3. Batch operations with toast feedback
  4. Four-zone layout visually organized
  5. Page size selector persists via localStorage
  6. Quality button disabled for non-valid proxies
  7. Concurrent limits enforce properly
  8. Dual badges display correctly

- [ ] **Step 5: Compile and commit**

  ```bash
  cargo build
  git add -A && git commit -m "feat(admin): page size config, polish, remove row dimming"
  ```

---

## Post-Implementation

After all tasks complete, create `docs/dev_notes/v0.37/summary.md` per DEV_NOTES_WORKFLOW.
