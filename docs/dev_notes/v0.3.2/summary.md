# v0.3.2 Summary — 端口配置、密码认证、ARM 构建优化

> 事实归档，不含质量评价。

## 交付内容

| 变更 | 说明 |
|------|------|
| 客户端端口配置 | 端口范围改为环境变量驱动 `PROXY_PORT_START`/`PROXY_PORT_END`（默认 60001-65535） |
| 密码认证 | 新增用户名/密码登录，管理员可通过后台创建/重置密码用户，与 OAuth 并存 |
| 前端适配 | 登录页新增密码登录表单；管理后台新增密码用户创建表单 + 来源标签列 |
| ARM 构建优化 | 原生交叉编译替代 QEMU 模拟，arm64 构建从 ~60min 降至 ~10min |
| 远程镜像配置 | 新增 `docker-compose-remote.yml`（server/client），支持直接拉取 DockerHub 镜像运行 |

## 涉及文件

| 区域 | 文件 |
|------|------|
| 客户端 | `sing-box-zenproxy/experimental/clashapi/server.go`、`docker/client/docker-compose.yml` |
| 认证 | `Cargo.toml`、`src/db.rs`、`src/api/auth.rs`、`src/api/admin.rs`、`src/api/mod.rs` |
| 前端 | `src/web/user.html`、`src/web/admin.html` |
| 构建 | `docker/server/Dockerfile`、`docker/client/Dockerfile` |
| 配置 | `docker/*/docker-compose-remote.yml`、`src/config.rs` |

## 验证记录

| 验证项 | 结果 |
|--------|------|
| `cargo check` | ✅ |
| `go build -tags "with_clash_api"` | ✅ |
| CI: Build Client Image (amd64+arm64) | ✅ |
| CI: Build Server Image (amd64+arm64) | ✅ (~10min) |
| 远程镜像拉取 + 启动（Client） | ✅ |
| 远程镜像拉取 + 启动（Server） | ✅ |

## 偏移记录

| 偏移 | 说明 |
|------|------|
| ARM 构建优化 | 原 plan 未包含，开发中发现 QEMU 构建耗时不可接受而追加 |
| `docker-compose-remote.yml` | 原 plan 未包含，开发中追加用于远程镜像测试 |
