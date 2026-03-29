# v0.38 Design — remote 客户端启动自动拉取与全量拉取

> Audience: AI / Dev
> Status: Locked
> 本文记录 v0.38 设计讨论最终共识，作为 plan 阶段输入。

---

## 1. 版本目标

v0.38 聚焦 remote 模式客户端的“启动即用”体验，目标是：

1. 客户端容器启动后自动从远端 ZenProxy Server 拉取代理，不再要求用户手动调用本地 `POST /fetch`
2. remote 模式默认开箱即启用自动拉取，但保留显式关闭开关
3. 服务端 `GET /api/client/fetch` 支持显式全量返回，避免客户端靠多次随机拉取拼凑“全量”
4. 自动拉取失败时不退出容器，只记录错误日志
5. 刷新方式在 v0.38 固定为“重启容器重新拉取”，不引入后台定时刷新

---

## 2. 用户可见行为

### 2.1 remote 模式默认行为

- `docker/client/docker-compose-remote.yml` 默认开启自动拉取
- 客户端 Clash API 启动成功后，后台异步执行一次 remote fetch
- 若启用自动绑定，拉取完成后本地端口立即可用
- 若拉取失败，容器保持存活，用户通过日志排查问题

### 2.2 刷新方式

- v0.38 不做在线刷新调度
- 用户通过重启容器触发重新拉取
- 每次启动自动拉取前，客户端先清理本地 `source=server` 的历史代理，避免 `store.json` 持续叠加重复数据

---

## 3. 服务端接口语义

### 3.1 `/api/client/fetch` 新增全量模式

服务端 `GET /api/client/fetch` 新增查询参数：

```text
all=true
```

语义：

- `all=true` 时忽略 `count`
- 返回所有满足筛选条件的代理
- `all=false` 或未提供时，保持现有 `count` 逻辑

### 3.2 现有状态模型的适配结论

客户端自动拉取**不需要额外适配**服务端新状态模型。

原因是当前 `/api/client/fetch` 最终复用代理池筛选逻辑，只会返回：

- `status == Valid`
- `is_disabled == false`
- `local_port.is_some()`

因此“有效/无效/待测试”与“启用/禁用”已在服务端完成过滤，客户端继续消费该接口即可。

---

## 4. 客户端自动拉取配置

### 4.1 环境变量

v0.38 为 remote 模式引入以下环境变量：

- `REMOTE_FETCH_ENABLED`
- `REMOTE_FETCH_SERVER`
- `REMOTE_FETCH_API_KEY`
- `REMOTE_FETCH_ALL`
- `REMOTE_FETCH_COUNT`
- `REMOTE_FETCH_COUNTRY`
- `REMOTE_FETCH_TYPE`
- `REMOTE_FETCH_CHATGPT`
- `REMOTE_FETCH_AUTO_BIND`
- `REMOTE_FETCH_SYNC_REMOTE_PORT`

### 4.2 默认值

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `REMOTE_FETCH_ENABLED` | `true` | remote 模式默认自动拉取 |
| `REMOTE_FETCH_ALL` | `false` | 默认不是全量拉取 |
| `REMOTE_FETCH_COUNT` | `10` | 仅在 `ALL=false` 时生效 |
| `REMOTE_FETCH_AUTO_BIND` | `true` | 拉取后默认立即创建本地绑定 |
| `REMOTE_FETCH_CHATGPT` | `false` | 默认不开启 ChatGPT 过滤 |
| `REMOTE_FETCH_COUNTRY` | 空 | 不限国家 |
| `REMOTE_FETCH_TYPE` | 空 | 不限协议类型 |
| `REMOTE_FETCH_SYNC_REMOTE_PORT` | 未设置 | 未设置时沿用现有 `SYNC_REMOTE_PORT` 全局逻辑 |

### 4.3 必填项

若开启自动拉取，则以下两个参数必须存在：

- `REMOTE_FETCH_SERVER`
- `REMOTE_FETCH_API_KEY`

缺失时只记录 warn / error 日志并跳过本次自动拉取，不中止 sing-box 进程。

---

## 5. `count` 与 `all` 的关系

### 5.1 最终规则

- `REMOTE_FETCH_ALL=true` 时，客户端请求服务端 `all=true`，并忽略 `REMOTE_FETCH_COUNT`
- `REMOTE_FETCH_ALL=false` 时，客户端按 `REMOTE_FETCH_COUNT` 发起请求
- 不使用 `count=all` 这类字符串魔法值

### 5.2 为什么需要服务端显式支持

当前服务端非全量模式是“过滤后随机抽样”，并非稳定分页或全量枚举。
因此若客户端自行循环请求，不仅会重复，还无法判断是否已拉全。
显式 `all=true` 是唯一清晰、可验证、可维护的全量语义。

---

## 6. 绑定策略

### 6.1 `REMOTE_FETCH_AUTO_BIND`

其意义是“拉取后是否立即把代理创建成本地可用端口”：

- `true`：启动后即可直接使用本地端口
- `false`：仅写入 store，不创建绑定，需要后续手动调用 `/bindings`

v0.38 默认值为 `true`，因为 remote 模式目标就是“部署后直接可用”。

### 6.2 端口同步

- 若 `REMOTE_FETCH_SYNC_REMOTE_PORT` 显式设置，则按本次自动拉取覆盖全局 `SYNC_REMOTE_PORT`
- 若未显式设置，则继续沿用现有 `SYNC_REMOTE_PORT`
- 同步模式仍保持“冲突即跳过、不回退自动分配”的既有策略

---

## 7. 数据替换策略

启动自动拉取前，只清理本地 `source=server` 的代理：

- 保留 `source=manual`
- 保留 `source=subscription`
- 不在 v0.38 自动清理这些本地来源

这样“重启容器 = 用远端最新快照替换旧的 server 来源代理”，同时不破坏本地手工数据。

---

## 8. 边界与不做事项

- 不把自动拉取配置写入 `config.json`
- 不做定时刷新、后台重拉或增量同步
- 不支持 `REMOTE_FETCH_COUNTRY` / `REMOTE_FETCH_TYPE` 的逗号多选
- 不修改服务端普通 `/api/fetch` 语义，v0.38 仅为客户端专用 `/api/client/fetch` 增加 `all=true`
- 不在自动拉取失败时退出容器
- 不把 remote 刷新扩展成独立管理接口；v0.38 以“重启容器刷新”为正式策略
