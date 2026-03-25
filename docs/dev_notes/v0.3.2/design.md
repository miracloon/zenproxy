# v0.3.2 Design

> **状态**：design confirmed，待写 implementation plan。

---

## 版本范围

v0.3.2 实现三项功能：

1. **CI 工作流验证** — 验证现有 GitHub Actions 双平台构建 + DockerHub 推送
2. **客户端端口配置增强** — 允许通过环境变量自定义代理池端口范围
3. **服务端密码认证** — 新增用户名/密码登录，管理员通过后台创建密码用户

---

## §1 CI 工作流验证

### 现状

`.github/workflows/docker.yml` 已配置完整：
- 触发条件：`push to main` 或 `push v* tag`
- 双平台：`linux/amd64` + `linux/arm64`
- 镜像 tag：`latest`（仅 main）、`v0.3.2`、`v0.3`（semver）
- 认证：`DOCKERHUB_TOKEN` + `DOCKERHUB_USERNAME` 已在 GitHub Actions secrets 配置

### 方案

不修改工作流配置。v0.3.2 功能实现完毕后：
1. 合并 `dev` → `main`
2. 打 `v0.3.2` tag 并推送
3. 观察 CI 构建是否成功
4. 验证 DockerHub 镜像是否正确推送（tag: `latest`, `v0.3.2`, `v0.3`）

---

## §2 客户端端口配置增强

### 现状

`server.go:96` 硬编码 `newPortPool(20001, 10000, ...)`，即端口范围 20001-30000。

`docker-compose.yml` 已使用 `PROXY_PORT_START` / `PROXY_PORT_END` 环境变量做宿主机端口映射，但未传入容器内部。

### 方案

**修改文件**：`sing-box-zenproxy/experimental/clashapi/server.go`

1. 新增 `getEnvUint16(key string, defaultVal uint16) uint16` 辅助函数
2. 修改 `NewServer()` 中 PortPool 初始化逻辑：
   ```go
   portStart := getEnvUint16("PROXY_PORT_START", 60001)
   portEnd := getEnvUint16("PROXY_PORT_END", 65535)
   portPool := newPortPool(portStart, portEnd-portStart+1, logFactory.NewLogger("port-pool"))
   ```
3. 客户端 `docker-compose.yml` 新增 `environment` 段，将 `PROXY_PORT_START` 和 `PROXY_PORT_END` 传入容器

**默认值**：60001-65535（与 INTENT.md 约定一致）

---

## §3 服务端密码认证

### 设计决策

- **多用户密码认证**：管理员通过 admin 后台 API 创建密码用户
- **与 OAuth 并存**：密码用户登录后获得与 OAuth 用户完全一致的 session / api_key
- **密码存储**：使用 `argon2` crate 做密码 hash
- **用户区分**：`auth_source` 字段标记来源（`oauth` / `password`）
- **信任等级**：密码用户的 `trust_level` 自动设为 `min_trust_level` 配置值

### DB 层改动

**文件**：`src/db.rs`

1. `users` 表新增字段：
   - `password_hash TEXT` — 可为 NULL（OAuth 用户无密码）
   - `auth_source TEXT NOT NULL DEFAULT 'oauth'` — 用户来源
2. DB 迁移：ALTER TABLE 添加新字段
3. 新增方法：
   - `get_user_by_username(username: &str)` — 按用户名查找
   - `create_password_user(username, password_hash)` — 创建密码用户
   - `update_user_password(user_id, password_hash)` — 重置密码

### API 层改动

**文件**：`src/api/auth.rs`、`src/api/mod.rs`、`src/api/admin.rs`

1. `POST /api/auth/login/password` — 密码登录端点
   - 请求体：`{ "username": "...", "password": "..." }`
   - 验证密码 → 创建 session → 设置 cookie → 重定向到 `/`
   - 失败返回 401

2. 管理 API（admin 路由下，需 admin_password 认证）：
   - `POST /api/admin/users/create` — 创建密码用户
     - 请求体：`{ "username": "...", "password": "..." }`
     - 自动生成 UUID id、api_key，设 `auth_source = "password"`
   - `PUT /api/admin/users/:id/password` — 重置用户密码
     - 请求体：`{ "password": "..." }`

3. Cargo.toml 新增依赖：`argon2`

### 前端层改动

**文件**：`src/web/user.html`

登录页原有 "使用 Linux DO 登录" 保留，新增：
- 分隔线 "—— 或 ——"
- 用户名输入框
- 密码输入框
- "密码登录" 按钮
- 调用 `POST /api/auth/login/password`，成功后 reload 页面

**文件**：`src/web/admin.html`

用户管理区域新增：
- "创建密码用户" 表单（用户名、密码）
- 调用 `POST /api/admin/users/create`
- 用户列表中显示 `auth_source` 标识

---

## 依赖关系

```
§2（端口配置）— 独立，可并行
§3（密码认证）— 独立，可并行
§1（CI 验证）— 依赖 §2 和 §3 完成后一起发布
```
