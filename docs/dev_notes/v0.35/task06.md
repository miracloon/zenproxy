# Task 06: Single Proxy Validation & Quality Check

**Depends on:** T01, T02, T05 (Disabled filtering)
**Blocking:** T07 (frontend)

---

## Goal

Add per-proxy validation and quality check endpoints. Handles both cases: proxy with existing binding (direct test) and proxy without binding (temporary allocation).

## Files

- Modify: `src/api/admin/proxies.rs` (new handlers)
- Modify: `src/api/mod.rs` (new routes)

---

## Steps

- [ ] **Step 1: Add single validate handler in `proxies.rs`**

```rust
pub async fn validate_single_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    if proxy.is_disabled {
        return Err(AppError::BadRequest("Cannot validate a disabled proxy".into()));
    }

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        let validation_url = state_clone.db.get_setting("validation_url")
            .ok().flatten()
            .unwrap_or_else(|| state_clone.config.validation.url.clone());
        let timeout = std::time::Duration::from_secs(
            state_clone.db.get_setting("validation_timeout_secs")
                .ok().flatten().and_then(|v| v.parse().ok())
                .unwrap_or(state_clone.config.validation.timeout_secs)
        );

        // Get or create binding
        let (local_port, temp_binding) = match state_clone.pool.get(&proxy_id) {
            Some(p) if p.local_port.is_some() => (p.local_port.unwrap(), false),
            Some(p) => {
                let mut mgr = state_clone.singbox.lock().await;
                match mgr.create_binding(&proxy_id, &p.singbox_outbound).await {
                    Ok(port) => (port, true),
                    Err(e) => {
                        tracing::error!("Failed to create temp binding for {}: {e}", proxy_id);
                        return;
                    }
                }
            }
            None => return,
        };

        // Run validation
        let proxy_addr = format!("http://127.0.0.1:{local_port}");
        let result = validate_through_proxy(&proxy_addr, &validation_url, timeout).await;

        match result {
            Ok(()) => {
                state_clone.pool.set_status(&proxy_id, crate::pool::manager::ProxyStatus::Valid);
                state_clone.db.update_proxy_validation(&proxy_id, true, None).ok();
                tracing::info!("Single validation OK: {}", proxy_id);
            }
            Err(e) => {
                state_clone.pool.set_status(&proxy_id, crate::pool::manager::ProxyStatus::Invalid);
                state_clone.db.update_proxy_validation(&proxy_id, false, Some(&e)).ok();
                tracing::info!("Single validation FAILED: {} — {e}", proxy_id);
            }
        }

        // Cleanup temp binding
        if temp_binding {
            let mut mgr = state_clone.singbox.lock().await;
            mgr.remove_binding(&proxy_id, local_port).await.ok();
        }
    });

    Ok(Json(json!({ "message": "Validation started for proxy" })))
}

/// Reusable single-proxy HTTP validation (same logic as validator.rs validate_single)
async fn validate_through_proxy(
    proxy_addr: &str,
    target_url: &str,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let proxy = reqwest::Proxy::all(proxy_addr).map_err(|e| format!("Proxy config error: {e}"))?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(proxy)
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| format!("Client build error: {e}"))?;

    let resp = client.get(target_url).send().await
        .map_err(|e| format!("Request failed: {e}"))?;

    if resp.status().is_success() || resp.status().is_redirection() {
        Ok(())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}
```

- [ ] **Step 2: Add single quality check handler in `proxies.rs`**

```rust
pub async fn quality_check_single_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    if proxy.is_disabled {
        return Err(AppError::BadRequest("Cannot quality-check a disabled proxy".into()));
    }

    if proxy.status != crate::pool::manager::ProxyStatus::Valid {
        return Err(AppError::BadRequest("Proxy must be valid before quality check. Run validation first.".into()));
    }

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        // Get or create binding
        let (local_port, temp_binding) = match state_clone.pool.get(&proxy_id) {
            Some(p) if p.local_port.is_some() => (p.local_port.unwrap(), false),
            Some(p) => {
                let mut mgr = state_clone.singbox.lock().await;
                match mgr.create_binding(&proxy_id, &p.singbox_outbound).await {
                    Ok(port) => {
                        state_clone.pool.set_local_port(&proxy_id, port);
                        (port, true)
                    }
                    Err(e) => {
                        tracing::error!("Failed to create temp binding for quality check {}: {e}", proxy_id);
                        return;
                    }
                }
            }
            None => return,
        };

        // Run quality check using the existing checker module
        // We re-fetch the proxy to get updated local_port
        if let Some(proxy) = state_clone.pool.get(&proxy_id) {
            let proxies = vec![proxy];
            let rate_limiter = std::sync::Arc::new(crate::quality::checker::SingleRateLimiter::new());
            crate::quality::checker::check_batch_public(&proxies, &state_clone, &rate_limiter).await;
            tracing::info!("Single quality check completed for {}", proxy_id);
        }

        // Cleanup temp binding
        if temp_binding {
            state_clone.pool.clear_local_port(&proxy_id);
            let mut mgr = state_clone.singbox.lock().await;
            mgr.remove_binding(&proxy_id, local_port).await.ok();
        }
    });

    Ok(Json(json!({ "message": "Quality check started for proxy" })))
}
```

> **Note:** The quality check reuses the existing `check_batch` logic from `checker.rs`. If `check_batch` is private, we need to either:
> 1. Make it `pub` (simplest)
> 2. Or extract the single-proxy check logic inline
>
> **Recommended approach:** Make `check_batch` in `checker.rs` `pub(crate)` so we can call it for a single-proxy vec. Or even simpler — just call `check_single` directly if we make it pub.

- [ ] **Step 3: Expose `check_single` from `checker.rs` (if needed)**

If the quality check handler needs to call checker internals, make `check_single` and `RateLimiter` pub(crate):

```rust
// In checker.rs, change:
// struct RateLimiter → pub(crate) struct RateLimiter
// async fn check_single → pub(crate) async fn check_single
// async fn check_batch → pub(crate) async fn check_batch
```

Or alternatively, keep the handler simple and just spawn a `check_all` targeting only that single proxy (less efficient but simpler).

**Simplest approach:** Make the single quality check handler trigger `check_all` but that checks all proxies. Instead, let's just inline the quality check logic similarly to validate — create the reqwest client through the proxy and run the checks manually. But this duplicates code.

**Practical approach:** Add `pub(crate) async fn check_single_proxy` to `checker.rs` that wraps the existing `check_single` + DB persistence:

```rust
// Add to checker.rs
pub(crate) async fn check_single_proxy(state: &Arc<AppState>, proxy_id: &str) -> Result<(), String> {
    let proxy = state.pool.get(proxy_id)
        .ok_or_else(|| "Proxy not found".to_string())?;

    if proxy.local_port.is_none() {
        return Err("Proxy has no active binding".into());
    }

    let local_port = proxy.local_port.unwrap();
    let proxy_addr = format!("http://127.0.0.1:{local_port}");
    let rate_limiter = RateLimiter::new(40);

    match check_single(&proxy_addr, &proxy, &rate_limiter).await {
        Ok(quality) => {
            let db_quality = ProxyQuality {
                proxy_id: proxy.id.clone(),
                ip_address: quality.ip_address.clone(),
                country: quality.country.clone(),
                ip_type: quality.ip_type.clone(),
                is_residential: quality.is_residential,
                chatgpt_accessible: quality.chatgpt_accessible,
                google_accessible: quality.google_accessible,
                risk_score: quality.risk_score,
                risk_level: quality.risk_level.clone(),
                extra_json: Some(serde_json::json!({"incomplete_retry_count": 0}).to_string()),
                checked_at: chrono::Utc::now().to_rfc3339(),
            };
            state.db.upsert_quality(&db_quality).ok();
            state.pool.set_quality(&proxy.id, quality);
            Ok(())
        }
        Err(e) => Err(e),
    }
}
```

- [ ] **Step 4: Simplify quality_check_single_proxy handler**

With the public helper from step 3:

```rust
pub async fn quality_check_single_proxy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let proxy = state.pool.get(&id)
        .ok_or_else(|| AppError::NotFound("Proxy not found".into()))?;

    if proxy.is_disabled {
        return Err(AppError::BadRequest("Cannot quality-check a disabled proxy".into()));
    }
    if proxy.status != crate::pool::manager::ProxyStatus::Valid {
        return Err(AppError::BadRequest("Proxy must be valid before quality check".into()));
    }

    let state_clone = state.clone();
    let proxy_id = id.clone();
    tokio::spawn(async move {
        // Ensure binding exists
        let temp_binding = if state_clone.pool.get(&proxy_id).map(|p| p.local_port.is_none()).unwrap_or(true) {
            if let Some(p) = state_clone.pool.get(&proxy_id) {
                let mut mgr = state_clone.singbox.lock().await;
                match mgr.create_binding(&proxy_id, &p.singbox_outbound).await {
                    Ok(port) => {
                        state_clone.pool.set_local_port(&proxy_id, port);
                        Some(port)
                    }
                    Err(e) => {
                        tracing::error!("Temp binding for quality check failed: {e}");
                        return;
                    }
                }
            } else { return; }
        } else { None };

        match crate::quality::checker::check_single_proxy(&state_clone, &proxy_id).await {
            Ok(()) => tracing::info!("Single quality check OK: {proxy_id}"),
            Err(e) => tracing::warn!("Single quality check failed for {proxy_id}: {e}"),
        }

        // Cleanup temp binding
        if let Some(port) = temp_binding {
            state_clone.pool.clear_local_port(&proxy_id);
            let mut mgr = state_clone.singbox.lock().await;
            mgr.remove_binding(&proxy_id, port).await.ok();
        }
    });

    Ok(Json(json!({ "message": "Quality check started for proxy" })))
}
```

- [ ] **Step 5: Register routes in `src/api/mod.rs`**

```rust
.route("/api/admin/proxies/:id/validate", post(admin::proxies::validate_single_proxy))
.route("/api/admin/proxies/:id/quality", post(admin::proxies::quality_check_single_proxy))
```

- [ ] **Step 6: Compile check + commit**

Run: `cargo build 2>&1 | head -30`

```bash
git add -A
git commit -m "feat(v0.35): single proxy validation and quality check endpoints"
```
