# v0.38 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 remote 模式客户端在容器启动后自动拉取远端代理并默认自动绑定，同时为服务端客户端专用 fetch 接口补齐显式全量拉取能力。

**Architecture:** 服务端在 `/api/client/fetch` 增加 `all=true` 语义，显式区分“随机抽样”与“全量返回”。客户端把现有 HTTP `POST /fetch` 的核心逻辑提炼成可复用函数，并在 Clash API 启动后按环境变量自动执行一次启动拉取；启动前仅清理 `source=server` 的历史代理，以“重启容器”作为刷新方式。

**Tech Stack:** Rust/Axum（server API）, Go/sing-box Clash API（client）, Docker Compose, Python smoke test

---

## File Structure

### Modified Files

| File | Tasks | Changes |
| --- | --- | --- |
| `src/api/fetch.rs` | T01 | `FetchQuery` 增加 `all` 参数，复用到客户端专用 fetch |
| `src/api/client_fetch.rs` | T01 | 为 `/api/client/fetch` 增加 `all=true` 分支与可测试选择逻辑 |
| `sing-box-zenproxy/experimental/clashapi/remote_fetch.go` | T02, T03 | `remoteFetchRequest` 增加 `all`，提炼共享 fetch 执行逻辑，供 HTTP 与启动自动拉取复用 |
| `sing-box-zenproxy/experimental/clashapi/server.go` | T03 | 在 Clash API 启动成功后触发一次异步 startup fetch |
| `sing-box-zenproxy/experimental/clashapi/store.go` | T03 | 增加按 `source` 清理 server 代理的能力 |
| `docker/client/docker-compose-remote.yml` | T04 | 增加 remote 自动拉取环境变量示例 |
| `README.md` | T04 | 补充 remote 自动拉取、`all=true`、刷新方式与验证说明 |

### New Files

| File | Tasks | Purpose |
| --- | --- | --- |
| `sing-box-zenproxy/experimental/clashapi/startup_fetch.go` | T03 | 启动自动拉取配置解析、启动 orchestration |
| `sing-box-zenproxy/experimental/clashapi/startup_fetch_test.go` | T03 | 覆盖默认值、开关、参数优先级、缺省容错 |
| `sing-box-zenproxy/experimental/clashapi/remote_fetch_test.go` | T02 | 覆盖请求参数构造、`all/count` 互斥语义、sync/auto-bind 结果整理 |
| `sing-box-zenproxy/experimental/clashapi/store_test.go` | T03 | 覆盖仅清理 `source=server` 的 replace 语义 |

---

## Task Carrier Decision

v0.38 使用单一 `plan.md` 承载任务边界，不再拆分 `taskNN.md`。

原因：

1. 本次变更以一条单链路为主：`服务端 all 语义 → 客户端共享 fetch → 启动自动拉取 → 部署文档`
2. 任务虽分属 Rust / Go / Docker / 文档，但依赖清晰、数量有限，继续拆成独立 task 文件收益不高
3. exec 阶段仍需严格按本计划中的任务边界推进，并在每个任务单元完成后先验证、再 commit

---

## Task Dependency Graph

```text
T01 (Server /api/client/fetch all=true)
  └─→ T02 (Client shared remote fetch + all/count)
        └─→ T03 (Startup auto-fetch + store replace)
              └─→ T04 (Compose / README / smoke guidance)
```

**Critical path:** T01 → T02 → T03 → T04

---

## Verification Strategy

1. Rust 侧优先使用单元测试锁定 `all=true` 语义，再运行 `cargo test`
2. Go 侧优先为共享 fetch / startup config 写失败测试，再运行 `go test ./experimental/clashapi`
3. 文档与部署模板改动至少运行一次 `docker compose -f docker/client/docker-compose-remote.yml config`
4. 最终手测以 remote 模式真实启动客户端、检查启动日志、确认 `/bindings` 与 `tests/smoke/client_check.py` 为准

---

## Detailed Tasks

### Task 01: 服务端客户端专用 fetch 增加 `all=true`

**Files:**
- Modify: `src/api/fetch.rs`
- Modify: `src/api/client_fetch.rs`

**What to do:**

- [ ] **Step 1: 先写失败测试，锁定 `all=true` 覆盖 `count` 的选择逻辑**

  在 `src/api/client_fetch.rs` 中先提炼一个可测试的纯选择函数，再为它写测试：

  - `all=true` 时返回所有满足筛选条件的代理
  - `all=true` 时忽略 `count`
  - `all=false` 时仍沿用随机抽样，只要求数量正确
  - disabled / invalid / untested / 无 `local_port` 的代理不得被客户端专用 fetch 返回

- [ ] **Step 2: 运行测试，确认先失败**

  Run: `cargo test client_fetch -- --nocapture`

  Expected: 因为 `all` 分支与测试辅助函数尚未实现而失败，失败原因应与新语义直接相关。

- [ ] **Step 3: 在查询结构中增加 `all` 参数**

  在 `src/api/fetch.rs` 的 `FetchQuery` 增加：

  ```rust
  #[serde(default)]
  pub all: bool,
  ```

  保持普通 `/api/fetch` 原语义不变，仅供 `/api/client/fetch` 复用查询结构。

- [ ] **Step 4: 在 `src/api/client_fetch.rs` 实现 `all=true` 分支**

  目标行为：

  - `proxy_id` 仍优先级最高
  - `all=true` 时走“全部候选”分支，不再调用 `pick_random`
  - `all=false` 时保留现有 `count.unwrap_or(10)` 抽样逻辑
  - 空结果返回结构保持兼容：`proxies=[]`, `count=0`, `message=...`

- [ ] **Step 5: 再跑测试确认通过**

  Run: `cargo test client_fetch -- --nocapture`

  Expected: 新增测试通过，且输出与 `all` 语义一致。

- [ ] **Step 6: 运行 Rust 全量测试确认无回归**

  Run: `cargo test`

  Expected: 全部通过。

- [ ] **Step 7: Commit**

  ```bash
  git add src/api/fetch.rs src/api/client_fetch.rs
  git commit -m "feat(server): support all=true in client fetch"
  ```

---

### Task 02: 客户端共享 remote fetch 核心逻辑并支持 `all`

**Files:**
- Modify: `sing-box-zenproxy/experimental/clashapi/remote_fetch.go`
- Create: `sing-box-zenproxy/experimental/clashapi/remote_fetch_test.go`

**What to do:**

- [ ] **Step 1: 先写失败测试，覆盖请求参数构造与优先级**

  在 `remote_fetch_test.go` 为将要提炼出的共享逻辑写测试，至少覆盖：

  - `all=true` 时请求 URL 包含 `all=true`，且不发送 `count`
  - `all=false` 时未提供 `count` 会回退到 `10`
  - `country` / `type` 仍保持单值传递，不支持逗号多选拆分
  - `sync_remote_port` 的请求值优先于全局 `SYNC_REMOTE_PORT`
  - 解析服务端返回时会把 `local_port` 映射为 `StoredProxy.RemotePort`

- [ ] **Step 2: 运行测试，确认先失败**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi -run 'TestRemoteFetch' -count=1
  ```

  Expected: 因共享执行函数与 `all` 语义尚未实现而失败。

- [ ] **Step 3: 为 HTTP handler 提炼共享 fetch 执行函数**

  在 `remote_fetch.go` 中将现有逻辑拆成可复用函数，例如：

  - 请求校验 / 默认值归一化
  - 远端 URL 构造
  - 服务端响应解析
  - 存储结果转换
  - auto-bind 执行

  要求：

  - HTTP handler 与启动自动拉取共用同一套核心逻辑
  - 不要把 startup 逻辑直接写死在 HTTP handler 中

- [ ] **Step 4: 在请求结构中增加 `All` 语义**

  `remoteFetchRequest` 增加：

  ```go
  All bool `json:"all"`
  ```

  行为：

  - `All=true` 时忽略 `Count`
  - `All=false` 且 `Count<=0` 时回退到 10

- [ ] **Step 5: 保持返回结构兼容**

  HTTP `POST /fetch` 仍返回：

  - `added`
  - `message`
  - `bound`（若 auto-bind）
  - `sync_errors`（若存在）

  只新增能力，不破坏原字段。

- [ ] **Step 6: 重新跑目标测试确认通过**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi -run 'TestRemoteFetch' -count=1
  ```

  Expected: 目标测试全部通过。

- [ ] **Step 7: Commit**

  ```bash
  git add sing-box-zenproxy/experimental/clashapi/remote_fetch.go sing-box-zenproxy/experimental/clashapi/remote_fetch_test.go
  git commit -m "feat(client): add all mode to remote fetch"
  ```

---

### Task 03: 客户端启动后自动拉取，并替换旧的 server 来源代理

**Files:**
- Modify: `sing-box-zenproxy/experimental/clashapi/server.go`
- Modify: `sing-box-zenproxy/experimental/clashapi/store.go`
- Modify: `sing-box-zenproxy/experimental/clashapi/remote_fetch.go`
- Create: `sing-box-zenproxy/experimental/clashapi/startup_fetch.go`
- Create: `sing-box-zenproxy/experimental/clashapi/startup_fetch_test.go`
- Create: `sing-box-zenproxy/experimental/clashapi/store_test.go`

**What to do:**

- [ ] **Step 1: 先写失败测试，锁定启动配置默认值与边界**

  在 `startup_fetch_test.go` 先为配置解析写测试，至少覆盖：

  - `REMOTE_FETCH_ENABLED` 默认 `true`
  - `REMOTE_FETCH_ALL` 默认 `false`
  - `REMOTE_FETCH_COUNT` 默认 `10`
  - `REMOTE_FETCH_AUTO_BIND` 默认 `true`
  - `REMOTE_FETCH_SYNC_REMOTE_PORT` 未设置时保持“无显式覆盖”
  - 缺少 `REMOTE_FETCH_SERVER` 或 `REMOTE_FETCH_API_KEY` 时返回“跳过启动拉取”的结果，而不是 panic / fatal
  - `REMOTE_FETCH_ALL=true` 时忽略 `REMOTE_FETCH_COUNT`

- [ ] **Step 2: 为 store 的 source 清理能力写失败测试**

  在 `store_test.go` 先锁定：

  - 仅删除 `Source == "server"` 的代理
  - `manual` / `subscription` 保留
  - 返回删除数量，便于 startup 日志记录

- [ ] **Step 3: 运行 Go 测试，确认先失败**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi -run 'TestStartupFetch|TestProxyStore' -count=1
  ```

  Expected: 因 startup 配置解析和按 source 清理能力尚未实现而失败。

- [ ] **Step 4: 实现 startup 配置解析与默认值**

  在 `startup_fetch.go` 中引入专用配置结构，职责限定为：

  - 读取环境变量
  - 归一化默认值
  - 判断是否启用启动拉取
  - 生成一次共享 remote fetch 请求

  设计要求：

  - 不把这组扩展配置塞进 `config.json`
  - `REMOTE_FETCH_ENABLED=false` 时直接跳过
  - 配置不完整时记录日志并跳过

- [ ] **Step 5: 在 store 中实现按 source 清理能力**

  在 `store.go` 增加仅清理 `source=server` 代理的函数，用于 startup refresh。

  约束：

  - 不清理 `manual`
  - 不清理 `subscription`
  - 不改现有 `DELETE /store` 的“清空全部”语义

- [ ] **Step 6: 在 `server.go` 的 `Start(adapter.StartStateStarted)` 接入异步 startup fetch**

  触发点要求：

  - 必须在 Clash API listener 启动成功之后
  - 使用 goroutine 异步执行，不阻塞 HTTP server 启动
  - 失败只写日志，不返回 error 中断整个客户端

- [ ] **Step 7: startup fetch 执行前做 replace-by-source**

  具体顺序：

  1. 读取 startup 配置
  2. 若禁用或配置不完整，则记录日志并返回
  3. 清理旧的 `source=server` 代理
  4. 调用 Task 02 提炼出的共享 fetch 执行函数
  5. 按 `AUTO_BIND` / `SYNC_REMOTE_PORT` 执行绑定
  6. 记录最终 `added` / `bound` / `sync_errors`

- [ ] **Step 8: 重新跑目标 Go 测试确认通过**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi -run 'TestRemoteFetch|TestStartupFetch|TestProxyStore' -count=1
  ```

  Expected: 新增测试全部通过。

- [ ] **Step 9: 跑一遍 package 级 Go 测试确认未破坏 clashapi**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi -count=1
  ```

  Expected: 通过。

- [ ] **Step 10: Commit**

  ```bash
  git add sing-box-zenproxy/experimental/clashapi/server.go \
          sing-box-zenproxy/experimental/clashapi/store.go \
          sing-box-zenproxy/experimental/clashapi/store_test.go \
          sing-box-zenproxy/experimental/clashapi/remote_fetch.go \
          sing-box-zenproxy/experimental/clashapi/startup_fetch.go \
          sing-box-zenproxy/experimental/clashapi/startup_fetch_test.go
  git commit -m "feat(client): auto fetch remote proxies on startup"
  ```

---

### Task 04: remote 部署模板、文档与冒烟验证说明

**Files:**
- Modify: `docker/client/docker-compose-remote.yml`
- Modify: `README.md`

**What to do:**

- [ ] **Step 1: 先补文档导向的失败检查**

  先列出必须在文档中说清楚的行为，防止实现后忘写：

  - remote 模式默认自动拉取
  - `REMOTE_FETCH_ALL` 与 `REMOTE_FETCH_COUNT` 的优先级
  - `REMOTE_FETCH_AUTO_BIND` 的含义
  - 刷新方式是“重启容器”
  - 自动拉取失败不退出容器
  - `tests/smoke/client_check.py` 如何用于部署后的端口验证

- [ ] **Step 2: 更新 `docker/client/docker-compose-remote.yml`**

  增加带默认值和注释的环境变量示例，至少包括：

  - `REMOTE_FETCH_ENABLED=true`
  - `REMOTE_FETCH_SERVER=...`
  - `REMOTE_FETCH_API_KEY=...`
  - `REMOTE_FETCH_ALL=false`
  - `REMOTE_FETCH_COUNT=10`
  - `REMOTE_FETCH_AUTO_BIND=true`
  - 可选的 `REMOTE_FETCH_COUNTRY`
  - 可选的 `REMOTE_FETCH_TYPE`
  - 可选的 `REMOTE_FETCH_CHATGPT`
  - 可选的 `REMOTE_FETCH_SYNC_REMOTE_PORT`

- [ ] **Step 3: 更新 `README.md`**

  需要补齐：

  - `/api/client/fetch` 的 `all=true` 语义
  - `POST /fetch` 的 `all` 字段
  - remote compose 的自动拉取部署说明
  - “重启容器即刷新”的正式说明
  - `client_check.py` 的使用位置与注意事项

- [ ] **Step 4: 验证 compose 模板可解析**

  Run:

  ```bash
  docker compose -f docker/client/docker-compose-remote.yml config
  ```

  Expected: 配置可正常展开，无语法错误。

- [ ] **Step 5: 最终手动验证脚本**

  在真实 remote 环境按以下路径手测：

  1. `docker compose -f docker/client/docker-compose-remote.yml up -d`
  2. 查看客户端日志，确认启动后自动发起一次 remote fetch
  3. `curl http://127.0.0.1:9090/bindings`（若配置了 secret 则补 Bearer）确认已出现绑定
  4. 在默认自动分配模式下，优先使用 `127.0.0.1:60001` 运行 `tests/smoke/client_check.py`
  5. 若开启 `REMOTE_FETCH_SYNC_REMOTE_PORT=true`，先从 `/bindings` 取一个实际端口，再修改 `client_check.py` 中的 `PROXY_PORT`

- [ ] **Step 6: Commit**

  ```bash
  git add docker/client/docker-compose-remote.yml README.md
  git commit -m "docs(client): document remote auto fetch deployment"
  ```

---

## Final Verification Before Summary

在 exec 阶段完成全部任务后，进入 summary 前至少执行：

```bash
cargo test
cd sing-box-zenproxy && go test ./experimental/clashapi -count=1
docker compose -f docker/client/docker-compose-remote.yml config
```

以及一次真实 remote 模式手测：

1. 启动容器
2. 检查 startup fetch 日志
3. 检查 `/bindings`
4. 跑 `tests/smoke/client_check.py`

只有这些验证结果明确后，才能进入 `summary.md`。
