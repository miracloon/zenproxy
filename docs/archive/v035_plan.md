# ZenProxy v0.35 Implementation Plan

This document outlines the detailed steps to implement the features discussed for v0.35, ensuring a smooth transition to RBAC, a modular admin dashboard, and granular proxy control.

## Phase 1: Database & Core Data Structures

### 1.1 Update Database Schema (`src/db.rs`)
- Add `role` column to `users` table:
  - Modify `Database::migrate` to `ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';`
  - Modify `User` struct to include `pub role: String`.
  - Update all user CRUD queries (`upsert_user`, `get_user_by_id`, etc.) to include `role`.
- Add `is_disabled` column to `proxies` table:
  - Modify `Database::migrate` to `ALTER TABLE proxies ADD COLUMN is_disabled INTEGER NOT NULL DEFAULT 0;`
  - Modify `ProxyRow` struct to include `pub is_disabled: bool`.
  - Update proxy queries (`insert_proxy`, `get_all_proxies`) to map `is_disabled`.

### 1.2 Configuration & Settings Cleanup (`src/config.rs` & `docker/`)
- Remove `admin_password` from `ServerConfig` and default configs.
- Rename generic OAuth settings to provider-specific ones (e.g., `enable_oauth` -> `linuxdo_oauth_enabled`).
- Update `docker/server/config/config.toml` template.

## Phase 2: Role-Based Authentication & Initialization

### 2.1 Initialization Logic (`src/main.rs`)
- Implement a startup check: Determine if any user exists with `role == "super_admin"`.
- If no `super_admin` exists (e.g., first run):
  - Automatically create a user with username `admin`, password `admin`, and role `super_admin`.
- (Frontend note: We will add a persistent warning banner if `admin/admin` is detected).

### 2.2 RBAC Auth Middleware (`src/api/auth.rs`, `src/api/mod.rs`)
- Remove the old `admin_auth` Bearer token middleware.
- Create two new session-based middlewares:
  - `require_admin`: Validates session cookie, checks if `user.role` is `admin` or `super_admin`.
  - `require_super_admin`: Validates session cookie, checks if `user.role` is `super_admin`.
- Apply `require_admin` to general admin routes.
- Apply `require_super_admin` to strictly sensitive routes (e.g., deleting a super_admin).

## Phase 3: Backend Admin Modularization

### 3.1 Refactor Admin Handlers (`src/api/admin/`)
- Split `src/api/admin.rs` into a module:
  - `src/api/admin/mod.rs` (Router setup)
  - `src/api/admin/users.rs` (User management, Role assignments)
  - `src/api/admin/proxies.rs` (Proxy toggle, Subscription edits)
  - `src/api/admin/settings.rs` (Config updates)

### 3.2 Granular Permissions for Users API
- **Delete User**:
  - Target: `super_admin` can delete anyone. `admin` can only delete `user`.
  - Safety Check: Prevent deletion of the *last* `super_admin`.
- **Change Role**:
  - Target: Endpoint `PUT /api/admin/users/:id/role`.
  - Rules: `admin` can only promote/demote `user` ↔ `admin`. `super_admin` can manage all role transitions.

### 3.3 Subscription Editing
- Add endpoint `PUT /api/subscriptions/:id` for modifying `name` and `url`.

## Phase 4: Proxy Management Enhancements

### 4.1 Proxy Toggle Implementation (`src/pool/manager.rs`, `src/api/admin/proxies.rs`)
- Add `is_disabled` to `PoolProxy`.
- Update `get_valid_proxies()` and routing logic to fully ignore proxies where `is_disabled == true`.
- Update `sync_proxy_bindings` to unbind ports for disabled proxies.
- Add endpoint `POST /api/admin/proxies/:id/toggle` to manually toggle `is_disabled`.

### 4.2 Individual Validation & Quality Check
- Add endpoint `POST /api/admin/proxies/:id/validate`.
- Add endpoint `POST /api/admin/proxies/:id/quality`.
- **Logic for individual checks**:
  - Check if `proxy.local_port` is `Some`. 
  - If Yes: Use existing port to run validation/quality task immediately.
  - If No: 
    - Rent a temporary port using `singbox_manager.create_binding()`.
    - Run the HTTP target check / ip-api check.
    - Release bind using `singbox_manager.remove_binding()`.

## Phase 5: Frontend Refactoring (`web/admin.html`)

### 5.1 Tabbed Interface
- Reorganize the monolithic dashboard into `div` containers toggled by URL hash:
  - `#subscriptions` (Default Tab)
  - `#users`
  - `#settings`

### 5.2 User Role Management UI
- Replace API Key / Trust logic table fields with visual `<select>` for Role assignment.
- Hide management actions if the viewing user lacks privilege.
- Conditionally render "Admin Dashboard" button on the main page *only* if the logged-in user is `admin` or higher.

### 5.3 Modular Settings UI
- Separate "Core Settings", "Validation Settings", and "OAuth Modules" into distinct visual cards.

### 5.4 Proxy Interactivity
- Add "Disable/Enable" toggle button on proxy rows.
- Add "Validate" and "Quality Check" action buttons with async loading states (spinner) specifically to individual proxy details/cards.
