# ZenProxy - 运转拓扑

> 本文档回答"项目如何运转、组件处于什么环节、影响什么上下游"。  
> 它描述的是运转逻辑，而不是操作步骤；具体命令、参数和使用手册应写在 `README.md` 中。

## 全链路概览

ZenProxy 的运转链路从本地开发到最终用户使用，经过以下主要环节：

```
本地开发 → 本地 Docker 构建测试 → 合并到 main → push →
GitHub Actions 编译（amd64 + arm64）→ 推送镜像到 DockerHub →
VPS / 本地拉取镜像 → 配置 → 导入订阅源 → 开始使用
```

运行态下（VPS）：

```
订阅源 / 手动导入 → ZenProxy Server（代理池管理、验证、质检、端口绑定）
                         ↓
                   下游程序使用 127.0.0.1:(Server 代理池端口) 各端口
```

运行态下（本地 Fetch）：

```
ZenProxy Server（VPS）→ Fetch API → sing-box-zenproxy（本地端口绑定）
                                          ↓
                                    下游程序使用 127.0.0.1:(Client 代理池端口) 各端口
```

---

## 运转环节总表

| 环节 | 主要组件 | 运行环境 | 输入 | 输出 | 下游影响 |
| --- | --- | --- | --- | --- | --- |
| 本地开发 | Rust / Go 源码 | 本地开发机 | 代码变更 | 编译产物 | Docker 构建测试 |
| 本地测试 | Docker | 本地开发机 | Dockerfile + 源码 | 可运行的容器 | 合并到 main |
| CI/CD | GitHub Actions | GitHub | main 分支 push | Docker 镜像（amd64 + arm64） | DockerHub |
| 部署 | Docker Compose | VPS / 本地 | 镜像 + 配置文件 | 运行中的服务 | 用户使用 |
| 代理导入 | ZenProxy Server API | VPS | 订阅 URL / 手动添加 | 代理池数据（SQLite） | 验证与质检 |
| 验证 | ZenProxy Server | VPS | 代理池中的代理 | Valid / Invalid 标记 | 端口绑定 |
| 质检 | ZenProxy Server | VPS | 已验证代理 | IP 信息、风险评分等 | 按条件筛选 |
| 端口绑定 | sing-box-zenproxy | VPS / 本地 | 代理信息（Fetch 或本地导入） | 本地代理端口池 | 下游程序 |

---

## 环节详解

### 本地开发

- **目的**：修改服务端或客户端代码
- **涉及组件**：`src/`（Rust 服务端）、`sing-box-zenproxy/`（Go 客户端）
- **运行位置**：本地开发机
- **对接关系**：开发完成后本地构建 Docker 镜像进行测试

### CI/CD 构建

- **目的**：自动编译多架构镜像并推送到 DockerHub
- **触发条件**：`main` 分支 push
- **涉及组件**：`.github/workflows/`、`docker/server/Dockerfile`、`docker/client/Dockerfile`
- **运行位置**：GitHub Actions
- **输出**：两个 Docker 镜像
  - server 镜像（zenproxy + sing-box），标签：`latest` + 版本 tag
  - client 镜像（sing-box-zenproxy），标签：`latest` + 版本 tag
- **架构支持**：amd64、arm64

### 部署运行

- **VPS 部署**：仅运行 server 容器（`network_mode: host`）。Server 内嵌 sing-box，自动为有效代理创建端口绑定。配置文件和数据目录通过 volume mount 挂载。
- **本地部署**：仅运行 client 容器（`network_mode: host`）。从远程 Server Fetch 代理后在本地创建端口绑定。
- **数据持久化**：
  - Server：`data/zenproxy.db`（SQLite 数据库）通过 volume 持久化
  - Client：`data/store.json`（代理存储）通过 volume 持久化
  - Bindings 不持久化，容器重启后需重新创建

### 使用态（运行时）

**VPS 场景**：

1. 通过 Server Web 管理后台或 API 添加订阅源
2. Server 自动拉取、解析、验证代理并创建端口绑定
3. VPS 上的下游程序使用 `127.0.0.1:(Server 代理池端口)` 各端口作为代理

**本地 Fetch 场景**：

1. Client 通过 Fetch API 从远程 Server 拉取代理信息
2. Client 批量创建端口绑定
3. 本地下游程序使用 `127.0.0.1:(Client 代理池端口)` 各端口作为代理

---

## 外部系统与依赖

| 外部依赖 | 作用 | 运转位置 |
| --- | --- | --- |
| GitHub Actions | CI/CD 构建与镜像推送 | GitHub 云端 |
| DockerHub | 镜像存储与分发 | DockerHub 云端 |
| Linux.do OAuth | 用户认证（原有） | 外部服务 |
| ip-api.com / ipinfo.io | 代理质检（IP 信息、风险评分） | 外部 API |
| 订阅源 | 代理数据来源 | 外部 URL |

---

## 分支策略

| 分支 | 用途 | 与 CI/CD 的关系 |
| --- | --- | --- |
| `fetch` | 完全同步上游仓库 | 不触发 CI |
| `dev` | 日常开发 | 不触发 CI |
| `main` | 发布分支，合并 `dev` | push 触发 GitHub Actions 构建镜像 |

---

## 开发环节在整体中的位置

开发产物（代码变更）通过 `dev` → `main` 合并后触发 CI/CD，最终以 Docker 镜像形式交付给用户部署使用。本地开发时通过本地 Docker 构建进行功能验证。

==开发环节的 AI 协作方式、阶段边界和版本派生文档逻辑，详见 [`docs/dev_notes/DEV_NOTES_WORKFLOW.md`](docs/dev_notes/DEV_NOTES_WORKFLOW.md)。==

---
