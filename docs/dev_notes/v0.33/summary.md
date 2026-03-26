# v0.33 Summary

> 事实归档 — 仅记录实际交付内容、偏移、未完成项

---

## 版本定位

v0.33 实现混合配置管理系统（DB + config.toml + Admin UI），并增强认证流程（用户注册、OAuth 开关）。

## 执行概况

| 维度 | 数据 |
| --- | --- |
| 分支 | `dev`（基于 v0.3.2 / `main`） |
| Commit 数 | 4（含 design/plan 文档） |
| 代码变更 | 17 文件，+588 / -39 行 |
| 新增依赖 | `toml_edit`（格式保留 TOML 回写） |
| Docker 测试 | ✅ 通过（本地 buildx 构建） |

## 提交记录

| 序号 | SHA | 内容 |
| --- | --- | --- |
| 1 | `12b23b8` | 设计文档 + 实施计划 |
| 2 | `208aae7` | Phase 1 — settings 表、config seed/writeback、AppState 更新 |
| 3 | `15988e8` | Phase 2 — Settings API、Auth 增强、runtime 配置迁移、Admin UI 设置面板、登录页重构 |
| 4 | `d674c2c` | Phase 3 — Docker config.toml 可写（移除 `:ro`） |

## 交付清单

### 1. 数据库层 (`src/db.rs`)
- 新增 `settings` 表迁移（key-value 存储）
- 4 个 CRUD 方法：`get_setting`、`set_setting`、`get_all_settings`、`set_all_settings`

### 2. 配置管理 (`src/config.rs`)
- `ServerConfig` 新增 `allow_registration`、`enable_oauth` 字段
- `seed_settings_to_db()` — 启动时将 config.toml → DB 种子（不覆盖已有值）
- `write_settings_to_config()` — 保存时 DB → config.toml 原子回写（`toml_edit` 保留格式）

### 3. 应用状态 (`src/main.rs`)
- `AppState` 新增 `config_path` 字段
- 启动时调用 `seed_settings_to_db()`
- 后台任务（验证、质检、订阅刷新）间隔从 DB 读取

### 4. Settings API (`src/api/admin.rs`, `src/api/mod.rs`)
- `GET /api/admin/settings` — 管理员读取全部配置
- `PUT /api/admin/settings` — 管理员保存配置（DB + config.toml 回写）

### 5. 认证增强 (`src/api/auth.rs`, `src/api/mod.rs`)
- `GET /api/auth/options`（公开）— 返回 `allow_registration` / `enable_oauth` 状态
- `POST /api/auth/register` — 用户自主注册（受 `allow_registration` 开关控制）
- OAuth 登录入口受 `enable_oauth` 开关控制，关闭时返回 403
- `admin_auth` 中间件改为从 DB 读取 `admin_password`，支持运行时修改

### 6. 错误类型 (`src/error.rs`)
- 新增 `Forbidden`（403）和 `Conflict`（409）变体

### 7. 运行时配置迁移
- `src/pool/validator.rs` — 验证参数（concurrency、timeout、url、error_threshold）从 DB 读取
- `src/quality/checker.rs` — 质检 concurrency 从 DB 读取
- `src/api/subscription.rs` — batch_size 从 DB 读取
- 所有读取均有 `unwrap_or(config)` 降级

### 8. Admin UI (`src/web/admin.html`)
- 新增"系统设置"面板（admin 密码、信任等级、注册/OAuth 开关、OAuth 配置、验证/质检参数、订阅刷新间隔）
- "💾 保存设置"按钮触发 `PUT /api/admin/settings`
- 修改 admin 密码时自动提示确认并更新本地存储

### 9. 登录页 (`src/web/user.html`)
- 密码登录在上，OAuth 在下（之前相反）
- OAuth 区域和注册链接根据 `/api/auth/options` 动态显示/隐藏
- 注册表单独立界面，支持密码注册 + 自动登录

### 10. Docker 配置
- `docker-compose.yml` / `docker-compose-remote.yml` 移除 config.toml `:ro`
- `config.toml` 模板新增 `allow_registration` / `enable_oauth`

## 与计划的偏移

| 项目 | 计划 | 实际 |
| --- | --- | --- |
| validation_batch_size 字段名 | `batch_size` | 种子时使用 `validation_batch_size` 作为 DB key — 无功能差异 |
| 冒烟测试脚本 | 计划新增 Python 冒烟测试 | 未新增脚本，改为手动 curl + 浏览器验证 |
| Phase 执行方式 | 计划 3 agent 并行 | 实际单 agent 串行（任务间有依赖，不适合并行） |

## 未完成 / 跳过

| 项目 | 原因 |
| --- | --- |
| Python 冒烟测试脚本 (`tests/`) | 手动验证已覆盖，可后续补充 |
| 认证端点速率限制 | 用户决定依赖外部 WAF |
| 后端密码强度校验 | 用户明确不需要 |

## Docker 测试结果

| 测试项 | 结果 |
| --- | --- |
| 容器启动 + DB 种子 | ✅ `Seeded 17 settings` |
| `GET /api/auth/options` | ✅ 正确返回 |
| `GET /api/admin/settings` | ✅ 17 项完整 |
| `PUT /api/admin/settings` + config.toml 回写 | ✅ 格式保留 |
| 注册开关生效 | ✅ 开→201，关→403 |
| 重名注册拒绝 | ✅ `Username already exists` |
| 登录页布局 | ✅ 密码在上，OAuth 在下 |
| Admin 设置面板 | ✅ 所有字段加载、保存可用 |
