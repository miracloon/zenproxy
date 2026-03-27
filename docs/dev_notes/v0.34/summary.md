# v0.34 Summary

> 事实归档。不评价质量、不提改进建议。

## 交付概览

v0.34 实现了客户端与远端代理端口同步功能，同时完成了 Docker 部署模型的简化。

## 完成项

### 计划内任务

| Task | 内容 | 状态 | Commit |
|------|------|------|--------|
| 1 | Server API 返回 `local_port` | ✅ | `b9de199` |
| 2 | Server PortPool `allocate_specific` + `create_binding_on_port` | ✅ | `a4df9df` |
| 3 | Server 启动时复用旧端口 | ✅ | `c16cb85` |
| 4 | Client 读取 `SYNC_REMOTE_PORT` 环境变量 | ✅ | `fd2c729` |
| 5 | Client Fetch 实现 `sync_remote_port` + 无回退策略 | ✅ | `6065065` |
| 6 | SPEC 文档更新 | ✅ | `5e5cc4d` |

### 讨论阶段附带完成（非计划内、在 design 讨论中直接修改）

| 内容 | Commit |
|------|--------|
| Docker 全部切换 `network_mode: host` | `bdf7476` |
| 删除 `.env` 文件，环境变量硬编码到 docker-compose | `bdf7476` |
| INTENT.md 新增 host 网络模式设计决策 | `bdf7476` |
| INTENT.md 修正部署拓扑（VPS 不需要 Client） | `bdf7476` |
| INTENT.md 更新端口同步功能描述 | `bdf7476` |
| WORKFLOW.md 修正运行态拓扑和使用流程 | `bdf7476` |
| SPEC.md 移除 `.env`，添加网络模式说明 | `bdf7476` |

## 偏移记录

| 偏移 | 说明 |
|------|------|
| Task 3 未添加 `clear_all_proxy_local_ports_except` | plan 中设计了一个新的 DB 方法用于清理非绑定代理的旧端口，实际实现中用逐条 `update_proxy_local_port_null` 替代，逻辑等价但更简单 |
| Task 3 未调用 `db.clear_all_proxy_local_ports()` | 原方案在启动时全量清 DB，改为不清 DB（保留旧端口供复用），仅清内存态 |

## 未完成项

无。plan 中所有 6 个 Task 均已完成并提交。

## 修改文件索引

### Server 端（Rust）

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `src/api/client_fetch.rs` | 修改 | `client_proxy_to_json` 新增 `local_port` 字段 |
| `src/singbox/process.rs` | 修改 | PortPool 新增 `allocate_specific`；SingboxManager 新增 `post_binding`、`create_binding_on_port` |
| `src/main.rs` | 修改 | 启动流程改为复用 DB 旧端口重建绑定 |

### Client 端（Go）

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `sing-box-zenproxy/.../server.go` | 修改 | 读取 `SYNC_REMOTE_PORT` 环境变量 |
| `sing-box-zenproxy/.../bindings.go` | 修改 | BindingManager 新增 `syncRemotePort` 字段和 `createBindingDirect` 方法 |
| `sing-box-zenproxy/.../remote_fetch.go` | 修改 | serverProxy 新增 `LocalPort`，请求新增 `SyncRemotePort`，StoredProxy 新增 `RemotePort`，Fetch 流程分支 sync/normal 模式 |
| `sing-box-zenproxy/.../store.go` | 修改 | StoredProxy 新增 `RemotePort` 字段 |

### Docker / 文档

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `docker/server/docker-compose.yml` | 修改 | `network_mode: host`，移除 ports |
| `docker/server/docker-compose-remote.yml` | 修改 | 同上 |
| `docker/client/docker-compose.yml` | 修改 | 同上 |
| `docker/client/docker-compose-remote.yml` | 修改 | 同上 |
| `docker/server/.env` | 删除 | 环境变量迁入 docker-compose |
| `docker/client/.env` | 删除 | 同上 |
| `docs/INTENT.md` | 修改 | §2 端口同步描述、§关键设计决策第 5 条、§6 部署拓扑修正 |
| `docs/WORKFLOW.md` | 修改 | 运行态拓扑图、部署运行、使用态描述 |
| `docs/SPEC.md` | 修改 | 移除 `.env`、新增网络模式和端口同步模式章节 |

## 验证状态

| 验证项 | 结果 |
|--------|------|
| `cargo check` | ✅ 通过（1 个 pre-existing dead_code 警告） |
| `go build ./experimental/clashapi/...` | ✅ 通过 |
| 端到端冒烟测试 | ⚠️ 未执行（需部署环境） |
