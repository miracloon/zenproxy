# Task 07: Frontend — Admin Dashboard (admin.html)

**Depends on:** T02, T03, T04, T05, T06 (all backend APIs must exist)
**Blocking:** None (T08 can run in parallel)

---

## Goal

Major refactor of `admin.html`:
1. Remove password login overlay → session+role auth
2. Add Tab layout (Subscriptions & Nodes / Users / Settings)
3. Add subscription edit UI
4. Add proxy disable/enable toggle + single validate/quality buttons
5. Role management in user table
6. OAuth settings card with provider-specific labels
7. Each settings card has independent save button

## File

- Modify: `src/web/admin.html` (~823 lines → estimated ~1200 lines)

---

## Steps

- [ ] **Step 1: Replace auth flow**

Remove the password prompt overlay (`loginOverlay`). Replace with:

```javascript
// On page load
async function init() {
    try {
        const resp = await fetch('/api/auth/me');
        if (!resp.ok) {
            window.location.href = '/';
            return;
        }
        const data = await resp.json();
        currentUser = data;

        if (data.role === 'user') {
            document.getElementById('app').innerHTML = `
                <div class="access-denied">
                    <h2>权限不足</h2>
                    <p>您的账户角色为普通用户，无法访问管理后台。</p>
                    <a href="/">返回仪表盘</a>
                </div>`;
            return;
        }

        // User is admin or super_admin, load the dashboard
        loadDashboard();
    } catch (e) {
        window.location.href = '/';
    }
}
```

Remove all `localStorage.getItem('admin_token')` references. All API calls now use cookies (no Authorization header needed since session cookie is automatic).

- [ ] **Step 2: Add Tab bar HTML**

Replace the current monolithic structure with:

```html
<div id="app">
    <div class="tab-bar">
        <button class="tab active" data-tab="subscriptions" onclick="switchTab('subscriptions')">
            📡 订阅与节点管理
        </button>
        <button class="tab" data-tab="users" onclick="switchTab('users')">
            👥 用户管理
        </button>
        <button class="tab" data-tab="settings" onclick="switchTab('settings')">
            ⚙️ 系统设置
        </button>
        <a href="/" class="tab-link">← 返回仪表盘</a>
    </div>

    <div class="tab-content active" id="tab-subscriptions">
        <!-- Stats cards + Actions + Add subscription + Subscription table + Proxy table -->
    </div>
    <div class="tab-content" id="tab-users">
        <!-- User stats + Create user + User table -->
    </div>
    <div class="tab-content" id="tab-settings">
        <!-- General settings card + Linux.do OAuth card -->
    </div>
</div>
```

- [ ] **Step 3: Tab switching JS + hash routing**

```javascript
function switchTab(tabName) {
    document.querySelectorAll('.tab-content').forEach(el => el.classList.remove('active'));
    document.querySelectorAll('.tab').forEach(el => el.classList.remove('active'));
    document.getElementById('tab-' + tabName).classList.add('active');
    document.querySelector(`.tab[data-tab="${tabName}"]`).classList.add('active');
    window.location.hash = tabName;
}

// On load, check hash
function initTab() {
    const hash = window.location.hash.replace('#', '') || 'subscriptions';
    switchTab(hash);
}
```

CSS for the tab:
```css
.tab-bar { display: flex; gap: 4px; padding: 16px 20px; border-bottom: 1px solid var(--border); }
.tab { background: transparent; border: none; color: var(--text-secondary); padding: 8px 16px;
       border-radius: 8px; cursor: pointer; font-size: 14px; transition: all 0.2s; }
.tab.active { background: var(--primary); color: #fff; }
.tab:hover:not(.active) { background: var(--bg-secondary); }
.tab-content { display: none; padding: 20px; }
.tab-content.active { display: block; }
.tab-link { margin-left: auto; color: var(--text-secondary); text-decoration: none; padding: 8px 16px; }
```

- [ ] **Step 4: Move existing content into tabs**

Reorganize existing HTML sections into the three tab containers:
- **tab-subscriptions**: stats cards, action buttons (validate/quality/cleanup), add subscription form, subscription table, proxy table
- **tab-users**: user count card, create user form, user table
- **tab-settings**: settings panels

This is a cut-and-paste reorganization of existing elements.

- [ ] **Step 5: Subscription edit UI**

Add edit button to subscription table rows and inline edit form:

```javascript
function renderSubscriptionRow(sub) {
    return `<tr id="sub-${sub.id}">
        <td>${escapeHtml(sub.name)}</td>
        <td class="url-cell">${sub.url ? escapeHtml(sub.url) : '(manual)'}</td>
        <td>${sub.proxy_count}</td>
        <td>
            <button class="btn-sm btn-primary" onclick="editSubscription('${sub.id}')">编辑</button>
            <button class="btn-sm" onclick="refreshSubscription('${sub.id}')">刷新</button>
            <button class="btn-sm btn-danger" onclick="deleteSubscription('${sub.id}')">删除</button>
        </td>
    </tr>`;
}

async function editSubscription(id) {
    // Show inline edit form in the row
    const row = document.getElementById('sub-' + id);
    const sub = subscriptions.find(s => s.id === id);
    row.innerHTML = `
        <td><input id="edit-name-${id}" value="${escapeHtml(sub.name)}" class="input-sm"></td>
        <td><input id="edit-url-${id}" value="${sub.url || ''}" class="input-sm" style="width:100%"></td>
        <td>${sub.proxy_count}</td>
        <td>
            <button class="btn-sm btn-primary" onclick="saveSubscription('${id}')">保存</button>
            <button class="btn-sm" onclick="loadSubscriptions()">取消</button>
        </td>`;
}

async function saveSubscription(id) {
    const name = document.getElementById('edit-name-' + id).value;
    const url = document.getElementById('edit-url-' + id).value;
    const resp = await fetch('/api/subscriptions/' + id, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, url: url || null }),
    });
    if (resp.ok) {
        showToast('订阅源已更新');
        loadSubscriptions();
    } else {
        showToast('更新失败', 'error');
    }
}
```

- [ ] **Step 6: Proxy table — add disable/toggle and single validate/quality buttons**

In the proxy table rendering, add new action buttons:

```javascript
function renderProxyRow(proxy) {
    const statusBadge = proxy.is_disabled
        ? '<span class="badge badge-disabled">已禁用</span>'
        : `<span class="badge badge-${proxy.status}">${statusText(proxy.status)}</span>`;

    const toggleBtn = proxy.is_disabled
        ? `<button class="btn-xs btn-success" onclick="toggleProxy('${proxy.id}')">启用</button>`
        : `<button class="btn-xs btn-warning" onclick="toggleProxy('${proxy.id}')">禁用</button>`;

    const validateBtn = !proxy.is_disabled
        ? `<button class="btn-xs" onclick="validateSingle('${proxy.id}')">验证</button>`
        : '';
    const qualityBtn = !proxy.is_disabled && proxy.status === 'valid'
        ? `<button class="btn-xs" onclick="qualitySingle('${proxy.id}')">质检</button>`
        : '';

    return `<tr class="${proxy.is_disabled ? 'row-disabled' : ''}">
        <td>${escapeHtml(proxy.name)}</td>
        <td>${statusBadge}</td>
        <!-- ... other columns ... -->
        <td class="actions">
            ${toggleBtn} ${validateBtn} ${qualityBtn}
            <button class="btn-xs btn-danger" onclick="deleteProxy('${proxy.id}')">删除</button>
        </td>
    </tr>`;
}

async function toggleProxy(id) {
    await fetch('/api/admin/proxies/' + id + '/toggle', { method: 'POST' });
    loadProxies();
}
async function validateSingle(id) {
    await fetch('/api/admin/proxies/' + id + '/validate', { method: 'POST' });
    showToast('验证已启动');
}
async function qualitySingle(id) {
    await fetch('/api/admin/proxies/' + id + '/quality', { method: 'POST' });
    showToast('质量检测已启动');
}
```

CSS for disabled rows:
```css
.row-disabled { opacity: 0.5; }
.badge-disabled { background: #6b7280; color: #fff; }
```

- [ ] **Step 7: User table — role management**

Update user table to show role column and actions based on current user's role:

```javascript
function renderUserRow(user) {
    const roleSelect = canChangeRole(user)
        ? `<select onchange="changeRole('${user.id}', this.value)">
            <option value="user" ${user.role === 'user' ? 'selected' : ''}>用户</option>
            <option value="admin" ${user.role === 'admin' ? 'selected' : ''}>管理员</option>
            ${currentUser.role === 'super_admin'
                ? `<option value="super_admin" ${user.role === 'super_admin' ? 'selected' : ''}>超级管理员</option>`
                : ''}
           </select>`
        : roleLabel(user.role);

    const deleteBtn = canDelete(user)
        ? `<button class="btn-xs btn-danger" onclick="deleteUser('${user.id}')">删除</button>`
        : '';

    return `<tr>
        <td>${escapeHtml(user.username)}</td>
        <td>${roleSelect}</td>
        <td>${user.auth_source}</td>
        <td>${user.is_banned ? '🚫' : '✅'}</td>
        <td>${deleteBtn} ...</td>
    </tr>`;
}

function canChangeRole(user) {
    if (user.id === currentUser.id) return false;
    if (currentUser.role === 'super_admin') return true;
    if (currentUser.role === 'admin' && user.role !== 'super_admin') return true;
    return false;
}
function canDelete(user) {
    if (user.id === currentUser.id) return false;
    if (currentUser.role === 'super_admin') return true;
    if (currentUser.role === 'admin' && user.role === 'user') return true;
    return false;
}

async function changeRole(id, role) {
    const resp = await fetch('/api/admin/users/' + id + '/role', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ role }),
    });
    if (resp.ok) showToast('角色已更新');
    else showToast('更新失败', 'error');
    loadUsers();
}
```

- [ ] **Step 8: Settings tab — split into cards with renamed labels**

```html
<!-- General Settings Card -->
<div class="settings-card">
    <h3>通用设置</h3>
    <div class="form-group">
        <label>☑ 允许用户注册</label>
        <input type="checkbox" id="set-allow-registration">
    </div>
    <div class="form-group">
        <label>验证 URL</label>
        <input id="set-validation-url" placeholder="https://www.bing.com">
    </div>
    <!-- ... validation_timeout_secs, concurrency, interval, threshold, batch_size ... -->
    <!-- ... quality_interval, quality_concurrency ... -->
    <!-- ... subscription_auto_refresh ... -->
    <button class="btn btn-primary" onclick="saveGeneralSettings()">💾 保存通用设置</button>
</div>

<!-- Linux.do OAuth Card -->
<div class="settings-card">
    <h3>Linux.do OAuth</h3>
    <div class="form-group">
        <label>☑ 启用 Linux.do OAuth 登录</label>
        <input type="checkbox" id="set-linuxdo-enabled">
    </div>
    <div class="form-group">
        <label>Linux.do 最低信任等级</label>
        <input type="number" id="set-linuxdo-trust" min="0" max="4">
        <small>Linux.do 社区的用户信任等级，0~4</small>
    </div>
    <div class="form-group">
        <label>Client ID</label>
        <input id="set-linuxdo-client-id">
    </div>
    <div class="form-group">
        <label>Client Secret</label>
        <input id="set-linuxdo-client-secret" type="password">
    </div>
    <div class="form-group">
        <label>Redirect URI</label>
        <input id="set-linuxdo-redirect-uri">
    </div>
    <button class="btn btn-primary" onclick="saveLinuxDoSettings()">💾 保存 OAuth 设置</button>
</div>
```

JS for per-card save:
```javascript
async function saveGeneralSettings() {
    const settings = {
        allow_registration: document.getElementById('set-allow-registration').checked.toString(),
        validation_url: document.getElementById('set-validation-url').value,
        // ... all general keys ...
    };
    await fetch('/api/admin/settings', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
    });
    showToast('通用设置已保存');
}

async function saveLinuxDoSettings() {
    const settings = {
        linuxdo_oauth_enabled: document.getElementById('set-linuxdo-enabled').checked.toString(),
        linuxdo_min_trust_level: document.getElementById('set-linuxdo-trust').value,
        linuxdo_client_id: document.getElementById('set-linuxdo-client-id').value,
        linuxdo_client_secret: document.getElementById('set-linuxdo-client-secret').value,
        linuxdo_redirect_uri: document.getElementById('set-linuxdo-redirect-uri').value,
    };
    await fetch('/api/admin/settings', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
    });
    showToast('OAuth 设置已保存');
}
```

- [ ] **Step 9: Remove all `admin_token` / `Authorization: Bearer` usage**

Search admin.html for all `Authorization` header usage and remove them. All API calls now rely on cookie auth:

```javascript
// Old:
headers: { 'Authorization': 'Bearer ' + adminToken, 'Content-Type': 'application/json' }
// New:
headers: { 'Content-Type': 'application/json' }
// (no Authorization header needed — cookie is sent automatically)
```

- [ ] **Step 10: Add `is_disabled` to proxy list API response**

Verify that `list_proxies` in `proxies.rs` includes `is_disabled` field. Update the JSON output:
```rust
json!({
    // ... existing fields ...
    "is_disabled": p.is_disabled,
})
```

- [ ] **Step 11: CSS styling for new elements**

Add styles for:
- `.tab-bar`, `.tab`, `.tab-content` — tab navigation
- `.settings-card` — card container with border, padding, margin
- `.badge-disabled` — gray badge
- `.row-disabled` — dimmed row
- `.btn-xs` — extra small action buttons
- `.btn-warning` — yellow/orange for disable button
- `.input-sm` — small inline inputs for edit mode
- Responsive adjustments

- [ ] **Step 12: Commit**

```bash
git add src/web/admin.html
git commit -m "feat(v0.35): admin dashboard — tab layout, RBAC UI, proxy controls, OAuth card"
```
