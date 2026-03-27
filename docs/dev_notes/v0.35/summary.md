# v0.35 Summary

> 事实归档。不评价质量、不提改进建议。

## 交付概览

v0.35 将 ZenProxy 从单管理员密码模式升级为三级角色权限体系（super_admin / admin / user），完成管理后台 Tab 模块化重构、代理精细控制（禁用/启用、单体连通测试/质量检测）、订阅源编辑、OAuth 配置模块化，以及用户管理功能的全面增强。

## 完成项

### 计划内任务

| Task | 内容 | 状态 | Commit |
|------|------|------|--------|
| T01 | DB Schema + Config Foundation | ✅ | `7ef62c2` |
| T02 | Admin Module Split | ✅ | `13d8389` |
| T03 | RBAC Auth + Super Admin Init | ✅ | `abad519` |
| T04 | Subscription Editing | ✅ | `eef78c0` |
| T05 | Proxy Disable/Enable | ✅ | `f789401` |
| T06 | Single Proxy Validate/Quality | ✅ | `46be2c0` |
| T07 | Frontend Admin Dashboard | ✅ | `1cce2af` |
| T08 | Frontend User Dashboard | ✅ | `3de01f7` |

### 计划外补丁（用户反馈后追加）

| 内容 | Commit |
|------|--------|
| ban_user 增加自我封禁保护（后端 400 + 前端隐藏按钮） | `a79fb98` |
| 新增 `PUT /api/admin/users/:id/username` 改名接口 | `a79fb98` |
| DB 新增 `update_user_username` 方法 | `a79fb98` |
| 前端用户表新增「改名」「重置密码」按钮 | `a79fb98` |
| 代理操作列拆分为「连通测试」（蓝色）和「质量检测」 | `a79fb98` |
| 操作列改用 flex 布局 + spinner 动画进度反馈 | `a79fb98` |
| ban_user 执行后清除 auth_cache 强制下线 | `a79fb98` |

## 偏移记录

| 偏移 | 说明 |
|------|------|
| T05 未新增 `ProxyStatus::Disabled` 枚举变体 | design 中设计了独立 Disabled 变体，实际实现用 `is_disabled: bool` 字段+现有 status 共存，语义等价且改动更小 |
| T07 admin.html 完全重写而非增量修改 | plan 中描述为增量修改，实际文件差异过大（544 删 534 增），选择完整重写以保证一致性 |
| T08 未实现用户自助改密 | design 中提到 `PUT /api/auth/change-password` 为可选，实际未实现。改为在管理后台由管理员重置密码 |
| 计划外新增 username 修改功能 | plan 中无此项，用户反馈后补充实现 |
| 代理操作按钮语义调整 | plan 中为「验证」「质检」，用户反馈后改为「连通测试」「质量检测」以区分含义 |

## 未完成项

| 项 | 说明 |
|----|------|
| 用户自助改密端点 | `PUT /api/auth/change-password` 未实现，当前由管理员在后台重置 |

## 修改文件索引

### 新增文件

| 文件 | 说明 |
|------|------|
| `src/api/admin/mod.rs` | Admin 路由模块入口 + CurrentUser extractor |
| `src/api/admin/users.rs` | 用户管理 handlers（list, delete, ban, unban, create, reset password, change role, update username） |
| `src/api/admin/proxies.rs` | 代理管理 handlers（list, delete, cleanup, trigger validation/quality, toggle, single validate/quality） |
| `src/api/admin/settings.rs` | 系统设置 handlers（get_settings, update_settings, get_stats） |

### 删除文件

| 文件 | 说明 |
|------|------|
| `src/api/admin.rs` | 被拆分为 `admin/` 模块目录 |
| `docker/server/.env` | 环境变量迁入 docker-compose（v0.34 遗留清理） |
| `docker/client/.env` | 同上 |

### 修改文件

| 文件 | 说明 |
|------|------|
| `src/db.rs` | 新增 `role` 列迁移、`is_disabled` 列迁移、settings key 重命名迁移；新增 CRUD 方法（set_proxy_disabled, update_user_role, update_user_password, update_user_username, count_users_by_role, create_password_user 等） |
| `src/config.rs` | 移除 admin_password / min_trust_level / enable_oauth；OAuth section 重构为 `[oauth.linuxdo]`；seed/writeback 适配新 key 名 |
| `src/main.rs` | 启动时自动初始化 super_admin（admin/admin）；日志输出默认密码 |
| `src/api/mod.rs` | 路由注册改为 admin 模块；新增 admin_only middleware（session + role check）；注册所有新 API 路由 |
| `src/api/auth.rs` | `/api/auth/me` 响应新增 role 字段；`/api/auth/options` 改用 `linuxdo_oauth_enabled` key |
| `src/api/subscription.rs` | 新增 `update_subscription` handler（PUT） |
| `src/api/client_fetch.rs` | filter_proxies 跳过 is_disabled 代理 |
| `src/pool/manager.rs` | PoolProxy 新增 `is_disabled` 字段；新增 `set_disabled`、`clear_local_port` 方法 |
| `src/pool/validator.rs` | validate_all 跳过 is_disabled 代理 |
| `src/quality/checker.rs` | check_all 跳过 is_disabled 代理；暴露 `check_single_proxy` 公开方法 |
| `src/singbox/process.rs` | 端口分配相关适配（v0.34 遗留） |
| `src/web/admin.html` | 完全重写：Tab 布局、RBAC 用户管理 UI、代理操作按钮、OAuth 设置卡片、订阅编辑、spinner 进度反馈 |
| `src/web/user.html` | 管理后台入口条件化显示（role !== user）；默认账户警告横幅；OAuth key 名修正 |
| `docker/server/config/config.toml` | 移除 admin_password；`[oauth]` → `[oauth.linuxdo]` |
| `docker/server/docker-compose.yml` | 环境变量调整 |
| `docker/server/docker-compose-remote.yml` | 同上 |
| `docker/client/docker-compose.yml` | 同上 |
| `docker/client/docker-compose-remote.yml` | 同上 |

### 文档

| 文件 | 说明 |
|------|------|
| `docs/dev_notes/v0.35/design.md` | 版本设计文档 |
| `docs/dev_notes/v0.35/plan.md` | 执行计划 |
| `docs/dev_notes/v0.35/task01.md` ~ `task08.md` | 8 个任务详细文档 |
| `docs/archive/v035_plan.md` | 早期 brainstorm 草案归档 |
| `docs/INTENT.md` | 更新角色权限描述 |
| `docs/SPEC.md` | 更新 admin 模块目录结构 |
| `docs/WORKFLOW.md` | 更新认证流程描述 |

## 新增 API 端点索引

| Method | Path | 说明 |
|--------|------|------|
| POST | `/api/admin/proxies/:id/toggle` | 切换代理禁用/启用 |
| POST | `/api/admin/proxies/:id/validate` | 单代理连通测试 |
| POST | `/api/admin/proxies/:id/quality` | 单代理质量检测 |
| PUT | `/api/subscriptions/:id` | 编辑订阅源 |
| POST | `/api/admin/users/create` | 创建密码用户 |
| PUT | `/api/admin/users/:id/password` | 重置用户密码 |
| PUT | `/api/admin/users/:id/role` | 修改用户角色 |
| PUT | `/api/admin/users/:id/username` | 修改用户名 |
| POST | `/api/admin/users/:id/ban` | 封禁用户 |
| POST | `/api/admin/users/:id/unban` | 解封用户 |

## 验证状态

| 验证项 | 结果 |
|--------|------|
| `cargo build` | ✅ 通过（1 个 pre-existing dead_code 警告） |
| Docker CI 构建 | ✅ 已触发（push to main） |
| 端到端冒烟测试 | ⏳ 用户自行验证 |
