# 项目规范手册（SPEC）

> 本文档回答"项目如何被组织"。  
> 它负责文档边界、目录结构、模块边界、命名与维护规范；不替代 `INTENT.md` 解释设计意图，也不替代 `WORKFLOW.md` 解释项目运转拓扑。

## 文档层级与边界

| 文档 | 稳定性 | 写什么 | 不写什么 | 修改时机 |
| --- | --- | --- | --- | --- |
| `AGENTS.md` | 高 | 入口级约束、最小识别信息、主链导航 | 完整项目说明、完整流程正文、详细结构规范 | 入口规则变化、导航变化、硬性约束变化时 |
| `docs/INTENT.md` | 高 | 项目 why、边界、取舍、关键决策 | 操作步骤、结构细目、执行流水 | 长期意图、边界、关键决策变化时 |
| `docs/WORKFLOW.md` | 中高 | 运转拓扑、环节关系、运行环境、上下游 | 命令教程、详细结构规范、开发阶段细规则 | 运行链路、环境边界、外部依赖变化时 |
| `docs/SPEC.md` | 中高 | 目录结构、模块边界、文档边界、命名与维护规范 | 设计初心、操作教程、开发流水 | 结构、边界、组织规范变化时 |
| `docs/dev_notes/DEV_NOTES_WORKFLOW.md` | 高 | 正式开发态下的运行时协作协议 | 项目整体 why、整体运转拓扑 | 开发协作规则变化时 |
| `docs/dev_notes/<version>/...` | 低到中 | 某次版本 / 任务的设计、计划、执行归档与评审 | 项目级长期规则 | 版本推进时 |

---

## 文档维护规则

- 新结论出现时，先判断它属于**长期控制规则**还是**阶段性执行信息**。
- 长期意图、边界、取舍 → `INTENT.md`。
- 运转环节、运行环境、上下游 → `WORKFLOW.md`。
- 目录结构、模块边界、命名与组织规范 → `SPEC.md`。
- 正式开发态运行时阶段规则 → `DEV_NOTES_WORKFLOW.md`。
- 某次版本的具体执行信息 → `docs/dev_notes/<version>/...`。
- 允许说明性交叉，但必须始终存在明确的规范归属。

---

## 项目目录结构

```text
zenproxy/
├── src/                          # Rust 服务端源码
│   ├── main.rs                   # 入口
│   ├── config.rs                 # 配置加载
│   ├── db.rs                     # SQLite 数据库操作
│   ├── error.rs                  # 错误类型
│   ├── api/                      # REST API handlers
│   ├── parser/                   # 订阅格式解析器
│   ├── pool/                     # 代理池管理
│   ├── quality/                  # 质检模块
│   ├── singbox/                  # sing-box 进程管理与配置生成
│   └── web/                      # Web 前端（管理后台）
├── sing-box-zenproxy/            # Go 客户端（修改版 sing-box 1.13.0）
│   └── experimental/clashapi/    # ZenProxy 新增的代理管理功能（核心修改区）
│       ├── proxy_manage.go       # 代理存储 CRUD + PortPool
│       ├── store.go              # ProxyStore 持久化
│       ├── subscription.go       # 订阅管理
│       ├── remote_fetch.go       # 远程 Fetch
│       ├── bindings.go           # 增强版绑定管理
│       ├── server.go             # 路由注册（修改）
│       └── parser/               # URI / YAML / Base64 解析器
├── docker/                       # Docker 部署相关
│   ├── server/                   # 服务端镜像
│   │   ├── Dockerfile
│   │   ├── docker-compose.yml
│   │   ├── .env
│   │   ├── config/config.toml    # 服务端配置模板
│   │   └── data/                 # 持久化数据目录
│   └── client/                   # 客户端镜像
│       ├── Dockerfile
│       ├── docker-compose.yml
│       ├── .env
│       ├── config/config.json    # 客户端配置模板
│       └── data/                 # 持久化数据目录
├── tests/                        # 测试脚本（Python）
├── docs/                         # 控制文档
│   ├── INTENT.md                 # 项目意图
│   ├── WORKFLOW.md               # 运转拓扑
│   ├── SPEC.md                   # 结构与规范（本文件）
│   ├── _templates/               # 控制文档模板
│   └── dev_notes/                # 开发笔记
│       ├── DEV_NOTES_WORKFLOW.md  # AI 开发协作规范
│       ├── DEV_NOTES_WORKFLOW_COMMAND.md  # workflow command 编写规范
│       └── <version>/            # 版本级开发文档
├── .github/workflows/            # GitHub Actions CI/CD
├── Cargo.toml / Cargo.lock       # Rust 项目配置
├── .dockerignore                 # Docker 构建上下文排除
├── AGENTS.md                     # AI 入口文档
├── CLAUDE.md                     # Claude 专属补丁
└── README.md                     # 用户操作手册
```


---

## 模块边界与组织方式

### 核心模块划分

| 模块 | 语言 | 目录 | 职责 | 不负责 |
| --- | --- | --- | --- | --- |
| ZenProxy Server | Rust | `src/` | 代理池管理、验证、质检、认证、API、Web 后台 | 本地代理绑定、协议实现 |
| sing-box-zenproxy | Go | `sing-box-zenproxy/` | 本地代理存储、订阅管理、端口绑定、Fetch | 代理验证、质检、集中管理 |

### 客户端修改区域约束

sing-box-zenproxy 对 sing-box 的修改**仅限于** `experimental/clashapi/` 目录下的新增文件和 `server.go`、`bindings.go` 的修改。sing-box 其余目录（`protocol/`、`route/`、`transport/` 等）保持原版不动。

---

## 命名、约定与维护规范

### 配置文件

- 服务端配置：TOML 格式（`config.toml`）
- 客户端配置：JSON 格式（`config.json`，sing-box 原生格式）
- Docker 环境变量：`.env` 文件

### 端口约定

| 用途 | 默认端口 | 可配置 |
| --- | --- | --- |
| ZenProxy Server HTTP | 3000 | `config.toml` [server] port |
| sing-box Clash API | 9090 | `config.toml` [singbox] api_port / `config.json` |
| 服务端代理池端口 | 10002-10301 | `config.toml` [singbox] base_port + max_proxies |
| 客户端代理池端口 | 60001-65535（fork 默认） | 待实现配置化 |

---

## AI 容易踩坑的结构性边界

- **`sing-box-zenproxy/` 不是一个独立项目**：它是 sing-box 的 fork，module path 仍为 `github.com/sagernet/sing-box`。不要尝试改 `go.mod` 中的 module 声明。
- **Server 和 Client 的 PortPool 是两套独立实现**：Server 侧在 `src/singbox/process.rs`（Rust），Client 侧在 `experimental/clashapi/proxy_manage.go`（Go）。修改端口逻辑时需注意两边同步。
- **服务端的 `sing-box` 实例和客户端的 `sing-box-zenproxy` 不是同一个东西**：服务端使用的 sing-box 是由 zenproxy 以子进程方式启动的，客户端的 sing-box-zenproxy 是独立运行的修改版 sing-box。


---
