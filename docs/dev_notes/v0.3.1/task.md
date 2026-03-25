# v0.3.1 Task — Docker 镜像构建

## Task 1: Dockerfiles + .dockerignore
- [x] 1.1 创建 `.dockerignore`
- [x] 1.2 创建 `docker/server/Dockerfile`（三阶段：Go → Rust → Alpine runtime）
- [x] 1.3 创建 `docker/client/Dockerfile`（两阶段：Go → Alpine runtime）

## Task 2: Docker Compose + 配置模板
- [x] 2.1 创建 `docker/server/docker-compose.yml`
- [x] 2.2 更新 `docker/server/.env`
- [x] 2.3 创建 `docker/server/config/config.toml`（配置模板）
- [x] 2.4 创建 `docker/client/docker-compose.yml`
- [x] 2.5 更新 `docker/client/.env`
- [x] 2.6 创建 `docker/client/config/config.json`（配置模板）

## Task 3: GitHub Actions CI/CD
- [x] 3.1 创建 `.github/workflows/docker.yml`

## Task 4: 本地验证
- [x] 4.1 本地 `docker build` server 镜像成功
- [x] 4.2 本地 `docker build` client 镜像成功
- [x] 4.3 `docker compose up` 验证容器启动
- [x] 4.4 Commit 所有文件
