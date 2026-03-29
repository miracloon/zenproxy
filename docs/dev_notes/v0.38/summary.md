# v0.38 Summary

> 事实归档。不评价质量、不提改进建议。

## 交付概览

v0.38 完成了 remote 模式客户端的启动自动拉取链路：服务端 `GET /api/client/fetch` 支持显式 `all=true`，客户端 `POST /fetch` 复用共享 remote fetch 核心逻辑并支持全量模式，Clash API 启动后会异步执行一次 startup fetch，在写入前替换本地旧的 `source=server` 代理；同时补齐了 remote compose、README、smoke 脚本与 Python 依赖说明，方便按“重启容器即刷新”的正式路径做部署与验证。

## 完成项

### 计划内任务

| Task | 内容 | 状态 | Commit |
|------|------|------|--------|
| T01 | 服务端 `/api/client/fetch` 增加 `all=true` 语义 | ✅ | `38bb69f` |
| T02 | 客户端共享 remote fetch 核心逻辑并支持 `all` | ✅ | `9b37a0f` |
| T03 | 客户端启动自动拉取，并在拉取前替换旧的 `source=server` 代理 | ✅ | `914719a` |
| T04 | remote 部署模板、README 与部署说明补齐 | ✅ | `eb45ff3` |

### 执行收口补充

| 内容 |
|------|
| 新增 `pyproject.toml`，统一 `tests/examples` 与 `tests/smoke` 所需的 Python 依赖 |
| 新增 `tests/smoke/client_check.py`，作为 remote 模式部署后的端口连通性 smoke 脚本 |
| `README.md` 与 `tests/examples/README.md` 改为使用仓库内脚本的真实路径，并与 `uv` 工作流对齐 |
| 删除 `tests/examples/config.json`，改为在 `tests/examples/README.md` 中以内联最小配置示例指导用户自行创建 |
| `.gitignore` 增加 Python 虚拟环境、缓存与本地 superpowers 目录忽略规则 |

## 实际完成情况

### 服务端

- `FetchQuery` 增加 `all` 参数，保持普通 `/api/fetch` 现有语义不变
- `/api/client/fetch` 支持 `all=true` 覆盖 `count`
- `proxy_id` 仍保持最高优先级
- 客户端专用 fetch 继续过滤 disabled / invalid / untested / 无 `local_port` 的代理

### 客户端

- `remote_fetch.go` 提炼共享 fetch 执行逻辑，HTTP handler 与 startup fetch 共用同一条执行路径
- `remoteFetchRequest` 增加 `all` 字段；`all=true` 时忽略 `count`
- 解析服务端返回时继续把 `local_port` 映射为 `StoredProxy.RemotePort`
- Clash API 启动成功后异步触发 startup fetch，不阻塞 HTTP server 启动
- startup fetch 执行前会清理本地旧的 `source=server` 代理，保留 `manual` / `subscription`
- startup fetch 支持 `REMOTE_FETCH_ENABLED`、`REMOTE_FETCH_ALL`、`REMOTE_FETCH_COUNT`、`REMOTE_FETCH_AUTO_BIND`、`REMOTE_FETCH_SYNC_REMOTE_PORT` 等环境变量
- 配置不完整或自动拉取失败时仅记录日志，不退出客户端容器

### 文档与验证支撑

- `docker/client/docker-compose-remote.yml` 提供 remote 自动拉取相关环境变量示例
- `README.md` 明确 `/api/client/fetch?all=true`、`POST /fetch` 的 `all` 字段、remote 模式自动拉取、重启容器刷新和 smoke 验证路径
- 新增 `tests/smoke/client_check.py`，用于代理出口 IP 与目标站访问的基础连通性检查
- `tests/examples/README.md` 改为使用 `uv sync` 与仓库内脚本路径，避免依赖系统级 Python / pip

## 偏移记录

| 偏移 | 说明 |
|------|------|
| T04 原计划只列出 `docker/client/docker-compose-remote.yml` 与 `README.md` | 实际收口阶段额外补入 `pyproject.toml`、`tests/smoke/client_check.py`、`tests/examples/README.md`、`.gitignore`，用于把 remote smoke 验证与 Python 依赖管理落到仓库内可执行形态 |
| `tests/examples/config.json` 被移除 | 实际保留内联最小配置示例，不再维护单独样例文件 |

## 未完成项

| 项 | 说明 |
|----|------|
| 真实 remote 环境手测 | 当前工作区已完成自动验证与 smoke 脚本落库，但未在本地提供可用的 remote 服务端 / 客户端部署环境，因而未归档启动日志、`/bindings` 实测结果和 `client_check.py` 的真实运行结果 |
| `review.md` | 用户未显式触发 review 阶段，本次未产出 |

## 提交记录

```text
eb45ff3 docs(client): document remote auto fetch deployment
914719a feat(client): auto fetch remote proxies on startup
9b37a0f feat(client): add all mode to remote fetch
38bb69f feat(server): support all=true in client fetch
54af0b6 docs(v0.38): add design and implementation plan
```

## 验证状态

| 验证项 | 结果 |
|--------|------|
| `cargo test` | ✅ 通过（8 passed） |
| `cd sing-box-zenproxy && go test ./experimental/clashapi -count=1` | ✅ 通过 |
| `docker compose -f docker/client/docker-compose-remote.yml config` | ✅ 通过 |
| `uv sync` | ✅ 通过 |
| `uv run python -m py_compile tests/smoke/client_check.py tests/examples/parallel_proxy.py tests/examples/rotating_proxy.py tests/examples/parallel_relay.py tests/examples/rotating_relay.py` | ✅ 通过 |
| 真实 remote 环境手测 | ⏳ 未在当前工作区执行 |
