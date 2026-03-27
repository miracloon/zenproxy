# v0.35 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform ZenProxy from single-admin-password auth into three-tier RBAC, modularize admin dashboard into tabs, add proxy disable/enable and per-proxy validation, and restructure OAuth config for multi-provider readiness.

**Architecture:** Backend changes span DB schema (role + is_disabled columns), config struct cleanup (remove admin_password, rename OAuth keys to linuxdo-prefixed), auth middleware rewrite (Bearer password → session+role), admin.rs split into sub-modules. Frontend changes are within the existing inline HTML approach — admin.html gets tab layout, user.html gets conditional rendering.

**Tech Stack:** Rust/Axum (backend), SQLite (DB), vanilla HTML/CSS/JS (frontend), argon2 (password hashing)

---

## File Structure

### New Files
- `src/api/admin/mod.rs` — admin router + shared types (replaces `src/api/admin.rs`)
- `src/api/admin/users.rs` — user management handlers (list, delete, ban, role change, create, reset password)
- `src/api/admin/proxies.rs` — proxy management handlers (list, delete, cleanup, toggle, single validate/quality)
- `src/api/admin/settings.rs` — settings handlers (get, update)

### Modified Files (by task)

| File | Tasks | Changes |
|------|-------|---------|
| `src/db.rs` | T01 | Add `role` column migration, `is_disabled` column migration, new CRUD methods |
| `src/config.rs` | T01 | Remove `admin_password`/`min_trust_level`/`enable_oauth` from ServerConfig, rename OAuth keys, update seed/writeback |
| `docker/server/config/config.toml` | T01 | Remove `admin_password`, restructure `[oauth]` → `[oauth.linuxdo]` |
| `src/api/admin.rs` → `src/api/admin/` | T02 | Split into module directory |
| `src/api/mod.rs` | T02, T03 | Update `mod admin`, rewrite `admin_auth` middleware |
| `src/api/auth.rs` | T03, T01 | Add `role` to `/api/auth/me` response, update `extract_session_user` |
| `src/main.rs` | T03 | Add default super_admin initialization at startup |
| `src/api/subscription.rs` | T04, T05 | Add `update_subscription` handler, filter disabled in `sync_proxy_bindings` |
| `src/pool/manager.rs` | T05 | Add `Disabled` to `ProxyStatus`, add `is_disabled` to `PoolProxy`, update filters |
| `src/pool/validator.rs` | T05 | Filter disabled proxies in `validate_all` |
| `src/quality/checker.rs` | T05 | Filter disabled proxies in `check_all` |
| `src/web/admin.html` | T07 | Tab layout, role management UI, proxy toggle/validate buttons, OAuth card, subscription edit |
| `src/web/user.html` | T08 | Conditional admin button, default password warning banner |

---

## Task Dependency Graph

```
T01 (DB + Config) ──┬──→ T03 (RBAC Auth) ──→ T07 (Frontend admin.html)
                    │                          ↑
T02 (admin.rs split)┤                          │
                    ├──→ T04 (Subscription edit)┤
                    │                           │
                    ├──→ T05 (Proxy disable)  ──┤
                    │                           │
                    └──→ T06 (Single validate)──┘
                    
T03 ──→ T08 (Frontend user.html)
```

**Critical path:** T01 → T02 → T03 → T07 → T08

**Parallelizable after T01+T02:** T03, T04, T05, T06 are mutually independent.

---

## Task Summary

| Task | Name | Scope | Est. Steps |
|------|------|-------|------------|
| T01 | DB Schema + Config Foundation | DB migrations, config struct, settings key rename | 14 |
| T02 | Admin Module Split | Split admin.rs into sub-modules, no logic changes | 8 |
| T03 | RBAC Auth + Super Admin Init | Middleware rewrite, default admin, role APIs | 16 |
| T04 | Subscription Editing | PUT endpoint, DB method | 6 |
| T05 | Proxy Disable/Enable | ProxyStatus::Disabled, toggle API, filter logic | 12 |
| T06 | Single Proxy Validate/Quality | Per-proxy endpoints, temp binding logic | 10 |
| T07 | Frontend Admin Dashboard | Tab layout, all new UI elements | 12 |
| T08 | Frontend User Dashboard | Conditional admin button, password warning | 6 |

---

## Verification Strategy

This project has no automated test suite. Verification is done via:
1. `cargo build` — compilation check after each backend task
2. Manual API testing with `curl` commands (specified per task)
3. Visual inspection of frontend changes in browser
4. Docker build smoke test at the end (optional)

---

## Detailed tasks

See individual task files: `task01.md` through `task08.md`.
