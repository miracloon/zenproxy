# v0.34 设计文档：客户端与远端端口同步

## 目标

客户端 Fetch 远端代理后，可选择使用远端代理的 `local_port` 绑定到本地——使同一代理在 VPS 和本地拥有相同的端口号。

## 边界

- 属于 INTENT.md Fork 增量意图 §2（允许客户端端口与服务端端口同步配置）的实现。
- 不修改 sing-box 核心逻辑。
- 不引入新的持久化机制（Server 端口持久化为独立话题，不在本版本范围内）。

## 关键设计决策

### 1. 配置方式：环境变量默认值 + API 参数覆盖（方案 C）

- 环境变量 `SYNC_REMOTE_PORT`（默认 `false`）作为全局默认值。
- Fetch API 请求参数 `sync_remote_port` 可按次覆盖。
- 优先级：请求参数 > 环境变量 > 默认值 false。

### 2. 无回退策略

`sync_remote_port=true` 时：
- 远端端口可用 → 绑定到该端口。
- 远端端口冲突 → **报错，跳过此代理**，不回退到自动分配。
- 远端代理无 `local_port` → **报错，跳过此代理**。
- 批量 Fetch 部分成功：成功的照绑，失败的在响应中列出错误。

理由：用户选择端口同步就是要一致性保证，静默回退会破坏这一期望。

### 3. Sync 模式下绕过 PortPool

`sync_remote_port=true` 时，不使用客户端 PortPool 分配端口，直接用远端端口号创建绑定。
- `PROXY_PORT_START/END` 在 sync 模式下失效但可保留配置，不报错。
- 端口冲突由 sing-box 的 `net.Listen` 在运行时检测。

### 4. Server 端口稳定性：DB 复用旧端口

Server 重启时不再清空所有 `local_port`，而是尝试复用 DB 中保存的旧端口号重建绑定。
- 旧端口不在当前配置范围内 → 丢弃，顺序分配。
- 绑定失败 → 顺序分配。
- 新代理（无旧端口）→ 顺序分配。

### 5. Docker 部署：host 网络模式

容器统一使用 `network_mode: host`，不通过 Docker 端口映射。（已在本版本讨论中完成修改。）

## 需要修改的组件

| 端 | 组件 | 修改内容 |
|----|------|----------|
| Server | `src/api/client_fetch.rs` | `client_proxy_to_json` 返回 `local_port` 字段 |
| Server | `src/main.rs` | 启动时复用 DB 旧端口重建绑定 |
| Server | `src/singbox/process.rs` | PortPool 新增 `allocate_specific(port)` 方法 |
| Client | `sing-box-zenproxy/.../remote_fetch.go` | 识别 `sync_remote_port` 参数，使用远端端口绑定 |
| Client | `sing-box-zenproxy/.../server.go` | 读取 `SYNC_REMOTE_PORT` 环境变量 |
| Client | `sing-box-zenproxy/.../bindings.go` | 新增不经过 PortPool 的直接端口绑定路径 |

## 不做

- Server 侧端口长期持久化保证（超出范围的架构变更）
- Client 自动从远端获取端口范围
- Client 端 Web UI 上的 sync 开关（Fetch 是 API 行为）
