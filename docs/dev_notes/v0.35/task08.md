# Task 08: Frontend — User Dashboard (user.html)

**Depends on:** T03 (role field in `/api/auth/me`)
**Blocking:** None

---

## Goal

Two changes to user.html:
1. Conditionally show "管理后台" button only for admin/super_admin roles
2. Show default password warning banner for admin/admin users

## File

- Modify: `src/web/user.html`

---

## Steps

- [ ] **Step 1: Add role to currentUser check**

The `loadUser()` function calls `/api/auth/me`. Update the dashboard rendering to check role.

Find the "管理后台" link (currently around line 135):

```html
<!-- Old: always shown -->
<a href="/admin" class="btn btn-admin">管理后台</a>
```

Wrap it in a container that's conditionally rendered:

```javascript
// In loadUser() or wherever the dashboard header is rendered:
const adminLink = user.role !== 'user'
    ? '<a href="/admin" class="btn btn-admin">管理后台</a>'
    : '';
document.getElementById('admin-link-container').innerHTML = adminLink;
```

Or use CSS to hide:
```javascript
if (user.role === 'user') {
    const adminBtn = document.querySelector('.btn-admin');
    if (adminBtn) adminBtn.style.display = 'none';
}
```

- [ ] **Step 2: Add default password warning banner**

After loading user info, check if the username is 'admin' and show a warning:

```javascript
// In loadUser() callback, after getting user data:
if (user.username === 'admin' && user.auth_source === 'password') {
    const banner = document.createElement('div');
    banner.className = 'warning-banner';
    banner.innerHTML = '⚠️ 你正在使用默认账户，请尽快<a href="#" onclick="showChangePasswordDialog()">修改密码</a>和用户名。';
    document.getElementById('app').prepend(banner);
}
```

CSS for the banner:
```css
.warning-banner {
    background: linear-gradient(135deg, #f59e0b22, #ef444422);
    border: 1px solid #f59e0b44;
    border-radius: 12px;
    padding: 12px 20px;
    margin: 16px 20px 0;
    color: #f59e0b;
    font-size: 14px;
}
.warning-banner a { color: #f59e0b; text-decoration: underline; }
```

- [ ] **Step 3: (Optional) Add password change dialog**

A simple modal for the user to change their own password. This can be a stretch goal — the user can also use the admin panel to reset their password.

If implemented:
```javascript
async function showChangePasswordDialog() {
    // Show modal with old password + new password fields
    // Call PUT /api/auth/change-password (would need a new endpoint)
}
```

This requires a new backend endpoint `PUT /api/auth/change-password` which is NOT in the current plan scope. For v0.35, the warning banner just links to the admin panel where they can reset their password in the user management tab.

Alternative: make the banner text "请在管理后台 → 用户管理中修改密码":

```javascript
banner.innerHTML = '⚠️ 你正在使用默认账户。请前往 <a href="/admin#users">管理后台 → 用户管理</a> 修改密码。';
```

- [ ] **Step 4: Update auth_options response usage**

The login page uses `/api/auth/options` to determine which login methods to show. Update the key names if the response changed:

```javascript
// Old:
if (options.enable_oauth) { ... }
// New:
if (options.linuxdo_oauth_enabled) { ... }
```

Check the login form rendering in user.html and update any references to old key names.

- [ ] **Step 5: Visual polish**

- Ensure the admin button styling matches the existing design
- Ensure the warning banner has smooth appearance animation
- Test responsive layout

- [ ] **Step 6: Commit**

```bash
git add src/web/user.html
git commit -m "feat(v0.35): user dashboard — conditional admin button, default password warning"
```
