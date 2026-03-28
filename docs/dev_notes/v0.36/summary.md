# v0.36 Summary — Core Engine Refactor

## 版本目标

将代理管理从 status-driven 模型重构为 enabled/disabled-driven 端口分配模型，实现端口记忆、批量管理 API、权限强化和前端更新。

## 完成的任务

### T01: DB Schema Migration
- 新增 `disabled_at` 列（proxies 表）
- 新增 `port_retention_hours` 配置项（默认 24h）
- 实现 `clear_expired_port_memory()` 和 `get_disabled_with_ports()` 方法

### T02: Core Engine Refactor
- **移除 `SyncMode` 枚举**，`sync_proxy_bindings` 仅管理 enabled 代理
- **端口记忆**：禁用代理保留 `local_port` 在数据库中，清除 SingboxManager 绑定
- **两阶段验证**：
  - Phase 1：用现有端口验证 enabled 代理
  - Phase 2：用临时绑定验证 disabled 代理（不持久化端口）
- **Checker 简化**：仅处理 enabled + valid + has-port 代理
- **Toggle 改进**：使用 `try_lock` 避免验证冲突

### T03: Batch APIs + Cleanup Timer
- `POST /api/admin/proxies/enable-valid` — 批量启用所有有效但禁用的代理
- `POST /api/admin/proxies/disable-invalid` — 批量禁用所有无效但启用的代理
- `POST /api/admin/proxies/validate-disabled` — 后台验证禁用代理
- 每小时后台任务清理过期端口记忆

### T04: Auth & Permissions
- `/api/auth/me` 返回 `auth_source` 字段
- `PUT /api/auth/password` — 自助修改密码（仅密码认证用户）
- `update_username` 和 `reset_user_password` 强制 `super_admin` 权限
- 凭证变更后清除 auth cache

### T05: Favicon + UI Fixes
- `/favicon.ico` 和 `/icon.png` 路由（从 `data/` 目录读取）
- 三个 HTML 页面添加 favicon link tags
- 修复复选框 UI：移除 Unicode ☑，checkbox 放入 label 内
- 信任等级范围 max 4→3，提示文本 0~4→0~3

### T06: Frontend Updates
- Admin 代理表新增「端口」列（可排序）
- 三个批量操作按钮：一键启用有效、一键禁用无效、验证未启用
- 可复用 Modal 组件替换所有 `prompt()` 调用
- 改名/重置密码按钮仅 super_admin 可见
- User 仪表盘新增端口列
- 用户下拉菜单（替代静态用户名显示）
- 密码用户可自助修改密码（Modal + PUT /api/auth/password）

## 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 端口记忆 | DB 保留 `local_port`，SingboxManager 清除 | 重启安全，重新启用时端口不变 |
| 验证隔离 | 临时绑定验证 disabled 代理 | 不影响 enabled 代理的端口分配 |
| 批量操作 | 独立 API 端点 | 前端可独立调用，不依赖全量验证 |
| Modal 组件 | 内联 HTML + JS 闭包 | 无外部依赖，与现有暗色主题一致 |

## 提交记录

```
96154cf T06 frontend updates
5d7e919 T05 favicon endpoints, checkbox UI fix, trust level range fix
e1bb777 T04 auth improvements
4df10d3 T03 batch enable/disable/validate APIs + cleanup timer
2f196b7 T02 core engine refactor
9621033 T01 DB schema migration
d1c5a83 docs: v0.36 design and plan
```

## 已知事项

- `from_str_loose` 在 `parser/mod.rs` 中有一个未使用警告（pre-existing，非关键）
- Favicon 需要用户手动放置 `data/favicon.ico` 和 `data/icon.png` 文件
