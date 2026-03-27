# Task 04: Subscription Editing

**Depends on:** T01 (DB method), T02 (module structure)
**Blocking:** T07 (frontend)

---

## Goal

Add `PUT /api/subscriptions/:id` endpoint to allow editing subscription name and URL.

## Files

- Modify: `src/api/subscription.rs` (new handler)
- Modify: `src/api/mod.rs` (new route)
- Note: DB method `update_subscription` already added in T01 step 6

---

## Steps

- [ ] **Step 1: Add `UpdateSubscriptionRequest` struct and handler in `subscription.rs`**

After the `AddSubscriptionRequest` struct (line ~19):

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateSubscriptionRequest {
    pub name: Option<String>,
    pub url: Option<String>,
}

pub async fn update_subscription(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sub = state.db.get_subscription(&id)?
        .ok_or_else(|| AppError::NotFound("Subscription not found".into()))?;

    let new_name = req.name.unwrap_or(sub.name);
    let new_url = req.url.or(sub.url);

    state.db.update_subscription(&id, &new_name, new_url.as_deref())?;

    tracing::info!("Subscription '{}' updated (name='{}', url={:?})", id, new_name, new_url);
    Ok(Json(json!({
        "message": "Subscription updated",
        "subscription": {
            "id": id,
            "name": new_name,
            "url": new_url,
        }
    })))
}
```

- [ ] **Step 2: Register route in `src/api/mod.rs`**

Update the existing `/api/subscriptions/:id` route (currently only has `delete`) to also accept `put`:

```rust
// Old:
.route("/api/subscriptions/:id", delete(subscription::delete_subscription))
// New:
.route("/api/subscriptions/:id", delete(subscription::delete_subscription).put(subscription::update_subscription))
```

- [ ] **Step 3: Compile check**

Run: `cargo build 2>&1 | head -20`

- [ ] **Step 4: Test with curl**

```bash
# Assuming a subscription exists with id "test-sub-id"
curl -X PUT http://localhost:3000/api/subscriptions/test-sub-id \
  -H "Cookie: zenproxy_session=<session_id>" \
  -H "Content-Type: application/json" \
  -d '{"name": "New Name", "url": "https://new-url.com/sub"}'
```

Expected: `{"message":"Subscription updated","subscription":{...}}`

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(v0.35): subscription editing API — PUT /api/subscriptions/:id"
```
