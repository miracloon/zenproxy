# v0.3.1 Summary — Docker 镜像构建

> 事实归档，不含质量评价。

## 交付内容

### 新增文件（13 件，commit `8558209`）

| 文件 | 说明 |
|------|------|
| `.dockerignore` | Docker 构建上下文排除规则 |
| `docker/server/Dockerfile` | Server 三阶段构建（Go → Rust → Alpine） |
| `docker/client/Dockerfile` | Client 两阶段构建（Go → Alpine） |
| `docker/server/docker-compose.yml` | 服务端编排，端口由 `.env` 环境变量控制 |
| `docker/client/docker-compose.yml` | 客户端编排，代理池端口范围由 `.env` 控制（默认 60001-65535） |
| `docker/server/.env` | 服务端环境变量模板 |
| `docker/client/.env` | 客户端环境变量模板 |
| `docker/server/config/config.toml` | 服务端配置模板（Docker 路径适配） |
| `docker/client/config/config.json` | 客户端 sing-box 最小配置模板 |
| `.github/workflows/docker.yml` | CI：main push → 多架构构建 → DockerHub |
| `docs/dev_notes/v0.3.1/design.md` | 设计文档 |
| `docs/dev_notes/v0.3.1/plan.md` | 执行计划 |
| `docs/dev_notes/v0.3.1/task.md` | 任务清单 |

### 未修改的代码

未修改任何 Rust 或 Go 源代码。

## 验证记录

| 验证项 | 结果 |
|--------|------|
| Client 镜像本地构建 (amd64) | ✅ 成功 |
| Server 镜像本地构建 (amd64) | ✅ 成功（修复 `include_str!` 问题后） |
| Client 容器启动 | ✅ sing-box started, Clash API on 9090 |
| Server 容器启动 | ✅ zenproxy on 3000, sing-box found at /app/sing-box, API ready |
| UI 冒烟测试（用户页） | ✅ 显示登录入口 |
| UI 冒烟测试（管理后台） | ✅ 密码 `change-me` 登录成功，Dashboard 正常 |
| CI workflow 文件 | 仅语法审查，未在 GitHub 实测 |

## 偏移记录

| 偏移 | 说明 |
|------|------|
| Dockerfile 移除 `--platform=$BUILDPLATFORM` | 本地无 buildx，移除该指令以兼容 legacy builder。CI 用 buildx 自动处理多架构。 |
| 添加 `COPY README.md` 和 `COPY config.toml` | Rust `include_str!` 宏在编译时引用了这两个文件，原始 plan 未预见。 |
| sing-box build tags 仅用 `with_clash_api` | plan 已明确此决策，与上游 Dockerfile 不同。 |

## 未完成项

- CI workflow 尚未实测（需合并到 `main` 并配置 DockerHub secrets）
- arm64 架构镜像未本地验证（依赖 CI 的 QEMU）
