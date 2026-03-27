# v0.35 Design — 角色权限、管理后台模块化、代理精细控制

> Audience: AI / Dev
> Status: Locked
> 本文记录 v0.35 设计讨论最终共识，作为 plan 阶段的输入。

---

## 1. 版本目标

将 ZenProxy 从 "单管理员密码" 模式升级为 "三级角色体系"，同时：
- 管理后台转为 Tab 分视角
- 订阅源可编辑
- 代理可手动禁用/启用，可单独验证/质检
- OAuth 配置模块化（语义更清晰）
- settings key 重命名为 provider-specific 前缀

---

## 2. 三级角色模型

### 2.1 角色定义

| 角色 | 值 | 含义 |
|------|-----|------|
| super_admin | `"super_admin"` | 超级管理员，全部权限 |
| admin | `"admin"` | 管理员，管理代理池和普通用户 |
| user | `"user"` | 普通用户，仅查看和拉取代理 |

### 2.2 权限矩阵

| 功能 | super_admin | admin | user |
|------|:-----------:|:-----:|:----:|
| 仪表盘 + Fetch/Relay | ✅ | ✅ | ✅ |
| 查看 "管理后台" 入口 | ✅ | ✅ | ❌ |
| 订阅/代理管理 | ✅ | ✅ | ❌ |
| 系统设置 | ✅ | ✅ | ❌ |
| 查看用户列表 | ✅ | ✅ | ❌ |
| 创建用户 | ✅ | ✅ | ❌ |
| 删除 user 级别 | ✅ | ✅ | ❌ |
| 删除 admin 级别 | ✅ | ❌ | ❌ |
| 删除 super_admin（保留≥1） | ✅ | ❌ | ❌ |
| 改角色 → user/admin | ✅ | ✅ | ❌ |
| 改角色 → super_admin | ✅ | ❌ | ❌ |

### 2.3 初始化

- 启动时检查是否存在至少一个 `super_admin`
- 不存在则自动创建：username=`admin` password=`admin` role=`super_admin`
- 登录后不强制改密码，仪表盘顶部显示持久警告横幅

### 2.4 认证变更

- **移除** `admin_password` 机制（config.toml 字段、settings 表、Bearer token middleware）
- admin API 改为 session cookie + role ≥ admin 的 middleware
- middleware 将当前用户注入 request extensions，handler 内做细粒度权限检查

---

## 3. 管理后台 Tab 分视角

- 单 URI `/admin`，JS Tab 切换 + URL hash 定位
- 三个 Tab（按序）：
  1. `#subscriptions` — 订阅与节点管理（默认）
  2. `#users` — 用户管理
  3. `#settings` — 系统设置

---

## 4. 订阅源编辑

- `PUT /api/subscriptions/:id` body: `{ "name": "...", "url": "..." }`
- 编辑 URL 后不自动刷新，用户手动触发
- DB: `update_subscription(id, name, url)`

---

## 5. 代理禁用/启用

- `ProxyStatus` 新增 `Disabled` 变体
- DB: `proxies` 表新增 `is_disabled INTEGER DEFAULT 0`
- `Disabled` 代理不参与：sync_bindings、validate_all、check_all、filter_proxies
- API: `POST /api/admin/proxies/:id/toggle` — 切换 disabled 状态
- 启用后恢复原有 validation 状态（不重置为 Untested）

---

## 6. 单个代理验证/质检

- `POST /api/admin/proxies/:id/validate`
- `POST /api/admin/proxies/:id/quality-check`
- 有 local_port 的代理直接测试；无 port 的临时分配 → 测试 → 清理

---

## 7. OAuth 模块化 + Settings Key 重命名

### 7.1 前端

- 系统设置 Tab 分两个独立卡片：通用设置、Linux.do OAuth
- 每个卡片独立保存按钮
- "启用 OAuth 登录" → "启用 Linux.do OAuth 登录"
- "最低信任等级" → "Linux.do 最低信任等级"（说明：此为 Linux.do 社区的信任等级，0~4）

### 7.2 Settings Key 重命名（破坏式变更）

| 旧 key | 新 key |
|--------|--------|
| `enable_oauth` | `linuxdo_oauth_enabled` |
| `oauth_client_id` | `linuxdo_client_id` |
| `oauth_client_secret` | `linuxdo_client_secret` |
| `oauth_redirect_uri` | `linuxdo_redirect_uri` |
| `min_trust_level` | `linuxdo_min_trust_level` |

- config.toml `[oauth]` section 改为 `[oauth.linuxdo]`
- `ServerConfig` 中移除 `admin_password`、`min_trust_level`、`enable_oauth`
- 无需 migration（无现有用户），直接替换

---

## 8. 后端模块化

- `src/api/admin.rs` → `src/api/admin/mod.rs` + `users.rs` + `proxies.rs` + `settings.rs`
- 路由注册集中在 `admin/mod.rs`

---

## 9. 设计边界

- 不引入前端框架，继续 inline HTML + JS + CSS
- 不拆分 `db.rs`（本轮新增方法不多，暂可控）
- 不改 Fetch/Relay API 认证方式（保持 API key + session 双轨）
- user 角色可使用所有 Fetch/Relay/Proxies 查看 API
