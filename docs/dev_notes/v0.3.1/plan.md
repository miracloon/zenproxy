# v0.3.1 Plan — Docker 镜像构建

> 基于 `design.md` 锁定的方案执行。

## 执行基线

- 不修改 Rust / Go 源码
- 产出 7 个文件：2 个 Dockerfile、2 个 docker-compose.yml、2 个配置模板、1 个 GitHub Actions workflow
- 另需 1 个 `.dockerignore`

## 依赖与顺序

```
Task 1 (Dockerfiles) → Task 2 (compose + config) → Task 3 (CI/CD) → Task 4 (本地验证)
```

Task 1~2 无外部依赖，可在本地完成。  
Task 3 依赖 DockerHub secrets 配置（用户手动在 GitHub 设置）。  
Task 4 需要本地 Docker 环境。

## 完成标准

1. `docker build` 在本地对 server 和 client Dockerfile 均能成功 (amd64)
2. `docker compose up` 能启动容器且进程正常运行
3. CI workflow 文件语法正确（可通过 `actionlint` 或手工审查）
4. 所有文件已 commit 到 `dev` 分支

---

## Task 1: Dockerfiles + .dockerignore

### 1.1 创建 `.dockerignore`

**文件**：`/.dockerignore`

```dockerignore
.git
.github
.agents
.agent
docs
tests
target
*.md
!README.md
LICENSE
skills-lock.json
CLAUDE.md
AGENTS.md
```

### 1.2 创建 Server Dockerfile

**文件**：`docker/server/Dockerfile`

三阶段构建：
1. `golang:1.24-alpine` — 编译 sing-box（仅 `with_clash_api` tag）
2. `rust:1.86-alpine` — 编译 zenproxy（musl 静态链接）
3. `alpine:3.21` — 运行时

关键点：
- Go 阶段：COPY `sing-box-zenproxy/` 到构建目录，`CGO_ENABLED=0` 编译
- Rust 阶段：COPY `src/`、`Cargo.toml`、`Cargo.lock`，安装 `musl-dev`，`cargo build --release`
- Runtime：COPY 两个二进制到 `/app/`，设 WORKDIR 和 ENTRYPOINT
- 使用 `BUILDPLATFORM` / `TARGETOS` / `TARGETARCH` 支持多架构

### 1.3 创建 Client Dockerfile

**文件**：`docker/client/Dockerfile`

两阶段构建：
1. `golang:1.24-alpine` — 编译 sing-box-zenproxy
2. `alpine:3.21` — 运行时

---

## Task 2: Docker Compose + 配置模板

### 2.1 Server docker-compose.yml

**文件**：`docker/server/docker-compose.yml`

```yaml
services:
  zenproxy-server:
    build:
      context: ../../
      dockerfile: docker/server/Dockerfile
    image: ${DOCKERHUB_USERNAME}/zenproxy-server:latest
    container_name: zenproxy-server
    restart: unless-stopped
    volumes:
      - ./config/config.toml:/app/config.toml:ro
      - ./data:/app/data
    ports:
      - "${SERVER_PORT:-3000}:3000"
      - "${SINGBOX_API_PORT:-9090}:9090"
    environment:
      - RUST_LOG=${RUST_LOG:-zenproxy=info,tower_http=info}
```

### 2.2 Server .env

**文件**：`docker/server/.env`

```env
DOCKERHUB_USERNAME=your-dockerhub-username
SERVER_PORT=3000
SINGBOX_API_PORT=9090
RUST_LOG=zenproxy=info,tower_http=info
```

### 2.3 Server config.toml 模板

**文件**：`docker/server/config/config.toml`

使用 README 中的配置模板，路径调整为 Docker 容器内路径：
- `binary_path = "/app/sing-box"`
- `config_path = "data/singbox-config.json"`
- `path = "data/zenproxy.db"`

### 2.4 Client docker-compose.yml

**文件**：`docker/client/docker-compose.yml`

```yaml
services:
  zenproxy-client:
    build:
      context: ../../
      dockerfile: docker/client/Dockerfile
    image: ${DOCKERHUB_USERNAME}/zenproxy-client:latest
    container_name: zenproxy-client
    restart: unless-stopped
    volumes:
      - ./config/config.json:/app/config.json:ro
      - ./data:/app/data
    ports:
      - "${CLASH_API_PORT:-9090}:9090"
    # 代理池端口范围，按需映射
    # - "60001-65535:60001-65535"
```

### 2.5 Client .env

**文件**：`docker/client/.env`

### 2.6 Client config.json 模板

**文件**：`docker/client/config/config.json`

---

## Task 3: GitHub Actions CI/CD Workflow

**文件**：`.github/workflows/docker.yml`

### Workflow 设计

- **触发**：push to `main`，且含版本 tag `v*` 时也附加版本标签
- **Jobs**：
  - `build-server`：构建并推送 server 镜像
  - `build-client`：构建并推送 client 镜像
- **Steps**：
  1. Checkout
  2. Set up QEMU（多架构）
  3. Set up Docker Buildx
  4. Login to DockerHub
  5. Docker meta（生成 tags：latest + version）
  6. Build and push（platforms: linux/amd64, linux/arm64）

### Secrets 需求

用户需在 GitHub repo settings 中配置：
- `DOCKERHUB_USERNAME`
- `DOCKERHUB_TOKEN`

---

## Task 4: 本地验证

### 4.1 本地构建验证

```bash
# Server 镜像
docker build -f docker/server/Dockerfile -t zenproxy-server:test .

# Client 镜像
docker build -f docker/client/Dockerfile -t zenproxy-client:test .
```

验证：两者均应成功构建。

### 4.2 容器启动验证

```bash
# 先确保配置模板已就位
cd docker/server && docker compose up -d
docker compose logs -f  # 检查 zenproxy 和 sing-box 启动日志
docker compose down

cd ../client && docker compose up -d
docker compose logs -f  # 检查 sing-box 启动日志
docker compose down
```

### 4.3 CI Workflow 语法检查

```bash
# 如果安装了 actionlint
actionlint .github/workflows/docker.yml
```

或人工审查 YAML 语法。

### 4.4 Commit

```bash
git add -A
git commit -m "feat(docker): add Dockerfiles, compose, config templates, and CI workflow"
```
