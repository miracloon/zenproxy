# ZenProxy

代理池管理与转发服务。Rust 服务端（代理池管理、验证、质检、认证）+ Go 修改版 sing-box 客户端（本地代理存储、端口绑定、多 IP 并发）。Fork 项目，保持上游核心逻辑，做使用层增强。

---

## AI 启动规则

- 先读本文件，再按文档导航进入主链。
- 本文件只提供入口级约束、最小识别信息和导航；不在此展开完整项目说明。
- 若需要理解项目为什么这样设计，去读 [`docs/INTENT.md`](docs/INTENT.md)。
- 若需要理解项目如何运转，去读 [`docs/WORKFLOW.md`](docs/WORKFLOW.md)。
- 若需要理解结构、目录与边界，去读 [`docs/SPEC.md`](docs/SPEC.md)。
- 若已进入正式开发态或版本化任务推进，再读 [`docs/dev_notes/DEV_NOTES_WORKFLOW.md`](docs/dev_notes/DEV_NOTES_WORKFLOW.md)。
- 若只是一次性小任务或普通问答，不要因为看到开发相关内容就自动进入完整 dev workflow。

---

## 硬性约束

- 服务端使用 Rust（Cargo），不替换为其他语言或框架。
- 客户端基于 sing-box 1.13.0 (dev-next) 修改版，Go 语言，不跟进上游 sing-box 版本更新。
- 客户端修改仅限 `sing-box-zenproxy/experimental/clashapi/` 目录，不动 sing-box 核心。
- 不修改项目核心架构和核心业务逻辑（代理验证、质检、订阅刷新、协议解析等）。
- Python 虚拟环境使用 uv 管理，与项目目录一致，禁止使用系统级 Python。
- 部署方式为 Docker，不使用裸机二进制部署。

---

## 快速环境摘要

- **服务端入口**：`src/main.rs`
- **客户端修改区**：`sing-box-zenproxy/experimental/clashapi/`
- **Docker 部署**：`docker/server/`、`docker/client/`
- **冒烟测试**：`tests/`（Python）
- **分支策略**：`fetch`（同步上游）、`dev`（开发）、`main`（发布，触发 CI）

---

## 文档导航

| 文档 | 定位 | 什么时候读 |
| --- | --- | --- |
| [`docs/INTENT.md`](docs/INTENT.md) | 项目意图 | 需要理解项目目标、边界、取舍、Fork 增量意图时 |
| [`docs/WORKFLOW.md`](docs/WORKFLOW.md) | 运转拓扑 | 需要理解运行环境、CI/CD、部署流程、上下游关系时 |
| [`docs/SPEC.md`](docs/SPEC.md) | 结构与规范 | 需要理解目录结构、模块边界、命名与维护规则时 |
| [`docs/dev_notes/DEV_NOTES_WORKFLOW.md`](docs/dev_notes/DEV_NOTES_WORKFLOW.md) | AI 开发运行时协议 | 已进入正式开发态或版本化任务推进时 |

---
