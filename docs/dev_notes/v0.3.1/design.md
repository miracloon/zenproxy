# v0.3.1 Design — Docker 镜像构建

> **目标**：实现 INTENT.md「Fork 增量意图 §1 Docker 镜像与 CI/CD」所描述的能力——构建 server / client 两个独立 Docker 镜像，配套 docker-compose 与 CI/CD 流水线。

---

## 范围

本版本只做 **Docker 镜像构建与 CI/CD 流水线**，不涉及端口配置增强、认证扩展、Dashboard 等后续功能。

### 交付物

1. `docker/server/Dockerfile` — 多阶段构建 server 镜像（Rust zenproxy + Go sing-box）
2. `docker/client/Dockerfile` — 多阶段构建 client 镜像（Go sing-box-zenproxy）
3. `docker/server/docker-compose.yml` + `.env` — 服务端一键启动
4. `docker/client/docker-compose.yml` + `.env` — 客户端一键启动
5. `docker/server/config/config.toml` — 服务端配置模板
6. `docker/client/config/config.json` — 客户端配置模板
7. `.github/workflows/docker.yml` — GitHub Actions CI（多架构构建 + DockerHub 推送）

### 不做

- 不改 Rust/Go 源代码
- 不修改分支策略（仍由 main push 触发 CI）
- 不做 Helm Chart / K8s manifests

---

## 关键设计决策

### 1. 多阶段构建策略

**Server 镜像**需要同时包含 Rust 编译产物（zenproxy）和 Go 编译产物（sing-box）。采用三阶段构建：

| 阶段 | 基础镜像 | 作用 |
|------|---------|------|
| builder-go | `golang:1.24-alpine` | 编译 sing-box（with_clash_api tag） |
| builder-rust | `rust:1.86-alpine` | 编译 zenproxy（musl 静态链接） |
| runtime | `alpine:3.21` | 最终运行镜像，仅含两个二进制 + ca-certificates |

> Go 版本使用 1.24 以匹配 go.mod 中声明的 `go 1.24.7`。上游 Dockerfile 使用 1.25（尚未发布，是 dev-next 分支预设值）。

**Client 镜像**只需一个 Go 编译阶段 + runtime 阶段。

### 2. sing-box 构建简化

上游 Dockerfile 使用的 build tags 非常多（gvisor, quic, wireguard, utls, acme, tailscale, ccm, ocm 等）。ZenProxy 仅需 `with_clash_api` tag，因为：

- 只使用 Clash API 进行代理管理
- 不需要 gvisor、wireguard、tailscale 等网络栈能力
- 更少的 tags = 更小的二进制 + 更快的编译

### 3. 二进制共置（Server 镜像）

根据 `process.rs` 中 `which_singbox()` 的逻辑，sing-box 优先从 zenproxy 可执行文件同目录查找。因此 server 镜像中将两个二进制放在同一目录 `/app/`。

### 4. 配置与数据卷挂载

| 镜像 | 配置挂载 | 数据挂载 |
|------|---------|---------|
| server | `./config/config.toml:/app/config.toml` | `./data:/app/data` |
| client | `./config/config.json:/app/config.json` | `./data:/app/data` |

### 5. CI/CD 触发策略

- **触发条件**：push to `main` 分支（符合 WORKFLOW.md 定义）
- **镜像标签**：`latest` + 版本 tag（如果 push 的 commit 有 git tag `v*`）
- **架构**：amd64 + arm64（使用 `docker/build-push-action` + QEMU）
- **推送目标**：DockerHub

### 6. Rust 交叉编译

使用 `rust:1.86-alpine` + `musl-dev` 进行 alpine 原生编译（amd64/arm64 各自编译），避免 cross-compilation 的复杂性。GitHub Actions 的 QEMU 模拟会处理 arm64 编译。

---

## 风险与取舍

| 风险 | 应对 |
|------|------|
| Go 1.25 不存在 | go.mod 声明 `go 1.24.7`，使用 `golang:1.24-alpine` 镜像 |
| arm64 QEMU 编译 Rust 很慢 | 可接受，CI 不追求极速；后续可考虑 cargo-zigbuild 交叉编译 |
| Docker build context 很大（sing-box-zenproxy 含大量无关文件） | 使用 `.dockerignore` 排除不需要的文件 |
