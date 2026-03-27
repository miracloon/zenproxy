# Task 05: Proxy Disable/Enable

**Depends on:** T01 (is_disabled column + DB method), T02 (module structure)
**Blocking:** T07 (frontend)

---

## Goal

Add `Disabled` status to proxies, toggle API, and ensure disabled proxies are excluded from all automated processes.

## Files

- Modify: `src/pool/manager.rs` (ProxyStatus + PoolProxy + filter methods)
- Modify: `src/api/admin/proxies.rs` (toggle endpoint)
- Modify: `src/api/mod.rs` (new route)
- Modify: `src/pool/validator.rs` (filter disabled)
- Modify: `src/quality/checker.rs` (filter disabled)
- Modify: `src/api/subscription.rs` (filter disabled in sync_proxy_bindings)

---

## Steps

- [ ] **Step 1: Add `Disabled` variant to `ProxyStatus` in `manager.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyStatus {
    Untested,
    Valid,
    Invalid,
    Disabled,
}

impl ProxyStatus {
    pub fn sort_weight(self) -> u8 {
        match self {
            ProxyStatus::Valid => 0,
            ProxyStatus::Untested => 1,
            ProxyStatus::Invalid => 2,
            ProxyStatus::Disabled => 3,
        }
    }
}
```

- [ ] **Step 2: Add `is_disabled` field to `PoolProxy` in `manager.rs`**

```rust
pub struct PoolProxy {
    // ... existing fields ...
    pub quality: Option<ProxyQualityInfo>,
    pub is_disabled: bool,
}
```

- [ ] **Step 3: Update `load_from_db` in `manager.rs`**

When constructing PoolProxy from DB row, map `is_disabled`:

```rust
let status = if row.is_disabled {
    ProxyStatus::Disabled
} else if row.is_valid {
    ProxyStatus::Valid
} else if row.last_validated.is_some() {
    ProxyStatus::Invalid
} else {
    ProxyStatus::Untested
};
let proxy = PoolProxy {
    // ... existing fields ...
    status,
    is_disabled: row.is_disabled,
    // ...
};
```

- [ ] **Step 4: Add toggle methods to `ProxyPool` in `manager.rs`**

```rust
pub fn set_disabled(&self, id: &str, disabled: bool) {
    if let Some(mut proxy) = self.proxies.get_mut(id) {
        proxy.is_disabled = disabled;
        if disabled {
            proxy.status = ProxyStatus::Disabled;
        } else {
            // Restore to last known status based on validation state
            // If it was disabled before validation, treat as Untested
            proxy.status = ProxyStatus::Untested;
        }
    }
}
```

- [ ] **Step 5: Update `get_valid_proxies` and `filter_proxies` to exclude disabled**

In `get_valid_proxies`:
```rust
pub fn get_valid_proxies(&self) -> Vec<PoolProxy> {
    self.proxies
        .iter()
        .filter(|p| p.status == ProxyStatus::Valid && !p.is_disabled)
        .map(|p| p.value().clone())
        .collect()
}
```

In `filter_proxies`, add filter at the top:
```rust
.filter(|p| !p.is_disabled)
```

- [ ] **Step 6: Add toggle endpoint in `src/api/admin/proxies.rs`**

```rust
pub async fn toggle_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    let new_disabled = !proxy.is_disabled;
    state.pool.set_disabled(&id, new_disabled);
    state.db.set_proxy_disabled(&id, new_disabled)?;

    // If disabling, clear the local port binding
    if new_disabled {
        if let Some(port) = proxy.local_port {
            let mut mgr = state.singbox.lock().await;
            mgr.remove_binding(&id, port).await.ok();
            state.pool.clear_local_port(&id);
            state.db.update_proxy_local_port_null(&id).ok();
        }
    }

    let status_str = if new_disabled { "disabled" } else { "enabled" };
    tracing::info!("Proxy {} {} (name={})", id, status_str, proxy.name);
    Ok(Json(json!({
        "message": format!("Proxy {}", status_str),
        "is_disabled": new_disabled,
    })))
}
```

- [ ] **Step 7: Register route**

In `src/api/mod.rs`, add:

```rust
.route("/api/admin/proxies/:id/toggle", post(admin::proxies::toggle_proxy))
```

- [ ] **Step 8: Update `validate_all` in `validator.rs` to skip disabled**

In the main loop, after collecting `to_validate` (line ~55-60), add filter:

```rust
.filter(|p| p.local_port.is_some() && p.status == ProxyStatus::Untested && !p.is_disabled)
```

Also update the re-check logic (line ~19-31) to skip disabled:
```rust
.filter(|p| p.status == ProxyStatus::Valid && p.error_count > 0 && !p.is_disabled)
```

And the "stuck" check (line ~65-76):
```rust
.filter(|p| p.status == ProxyStatus::Untested && !p.is_disabled)
```

- [ ] **Step 9: Update `check_all` in `checker.rs` to skip disabled**

In the collection of `to_check` (line ~50-56):
```rust
.filter(|p| p.local_port.is_some() && !p.is_disabled)
```

And for the portless count check (line ~59-63):
```rust
.filter(|p| p.local_port.is_none() && needs_quality_check(p, &now) && !p.is_disabled)
```

- [ ] **Step 10: Update `sync_proxy_bindings` in `subscription.rs` to skip disabled**

In the proxy classification loop (line ~360-374), add `Disabled` arm:

```rust
for p in all_proxies {
    if p.is_disabled {
        // Disabled proxies never get ports
        if p.local_port.is_some() {
            state.pool.clear_local_port(&p.id);
            state.db.update_proxy_local_port_null(&p.id).ok();
        }
        continue;
    }
    match p.status {
        // ... existing match arms ...
    }
}
```

- [ ] **Step 11: Update all PoolProxy construction sites**

Search for all places that create `PoolProxy` and add `is_disabled: false`:
- `src/api/subscription.rs` line ~120 (add_subscription)
- `src/api/subscription.rs` line ~260 (refresh_subscription_core)

```bash
grep -n "PoolProxy {" src/
```

- [ ] **Step 12: Compile check + commit**

Run: `cargo build 2>&1 | head -30`

```bash
git add -A
git commit -m "feat(v0.35): proxy disable/enable — Disabled status, toggle API, automated process filtering"
```
