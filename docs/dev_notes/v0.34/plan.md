# v0.34 端口同步 Implementation Plan

> **For agentic workers:** 按 task 顺序执行，每个 task 完成后 commit。

**Goal:** 实现客户端 Fetch 时可选同步远端代理端口号，同时使 Server 端口在重启后保持稳定。

**Architecture:** Server API 新增返回 `local_port`，Server 启动流程改为复用旧端口，Client Fetch 流程新增 `sync_remote_port` 模式直接使用远端端口创建绑定。

**Tech Stack:** Rust (Server), Go (Client)

---

## Task 1: Server API 返回 local_port

**Files:**
- Modify: `src/api/client_fetch.rs:62-79`

- [ ] **Step 1: 在 `client_proxy_to_json` 中添加 `local_port` 字段**

```rust
fn client_proxy_to_json(p: &crate::pool::manager::PoolProxy) -> serde_json::Value {
    json!({
        "id": p.id,
        "name": p.name,
        "type": p.proxy_type,
        "server": p.server,
        "port": p.port,
        "outbound": p.singbox_outbound,
        "local_port": p.local_port,
        "quality": p.quality.as_ref().map(|q| json!({
            "country": q.country,
            "chatgpt": q.chatgpt_accessible,
            "google": q.google_accessible,
            "is_residential": q.is_residential,
            "risk_score": q.risk_score,
            "risk_level": q.risk_level,
        })),
    })
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check`
Expected: 编译通过，无新警告

- [ ] **Step 3: Commit**

```bash
git add src/api/client_fetch.rs
git commit -m "feat(server): return local_port in client fetch API"
```

---

## Task 2: Server PortPool 支持指定端口分配

**Files:**
- Modify: `src/singbox/process.rs:15-48`（PortPool 结构体和方法）

- [ ] **Step 1: 为 PortPool 添加 `allocate_specific` 方法**

```rust
/// Try to allocate a specific port. Returns Ok if the port is free and within range,
/// Err if occupied or out of range.
fn allocate_specific(&mut self, port: u16) -> Result<(), String> {
    if port <= self.base_port || port > self.base_port + self.max_ports {
        return Err(format!("port {} out of range ({}-{})",
            port, self.base_port + 1, self.base_port + self.max_ports));
    }
    if self.used.contains(&port) {
        return Err(format!("port {} already allocated", port));
    }
    self.used.insert(port);
    Ok(())
}
```

- [ ] **Step 2: 为 SingboxManager 添加 `create_binding_on_port` 方法**

```rust
/// Create a binding on a specific port (for restoring saved port assignments).
/// Falls back to regular allocate() if the specific port fails.
pub async fn create_binding_on_port(
    &mut self,
    proxy_id: &str,
    port: u16,
    outbound_json: &serde_json::Value,
) -> Result<u16, String> {
    // Try specific port first
    if self.port_pool.allocate_specific(port).is_ok() {
        match self.post_binding(proxy_id, port, outbound_json).await {
            Ok(()) => return Ok(port),
            Err(e) => {
                self.port_pool.free(port);
                tracing::warn!("Failed to restore port {port} for {proxy_id}: {e}, falling back to sequential");
            }
        }
    } else {
        tracing::warn!("Cannot restore port {port} for {proxy_id} (out of range or occupied), falling back");
    }
    // Fallback to sequential
    self.create_binding(proxy_id, outbound_json).await
}
```

- [ ] **Step 3: 提取 `post_binding` 内部方法**

将 `create_binding` 中的 HTTP POST 逻辑提取为 `post_binding`，供两个入口共用：

```rust
async fn post_binding(
    &self,
    proxy_id: &str,
    port: u16,
    outbound_json: &serde_json::Value,
) -> Result<(), String> {
    let url = format!("{}/bindings", self.api_base);
    let secret = self.config.api_secret.clone().unwrap_or_default();
    let payload = serde_json::json!({
        "tag": proxy_id,
        "listen_port": port,
        "outbound": outbound_json,
    });
    let result = self.client
        .post(&url)
        .bearer_auth(&secret)
        .json(&payload)
        .send()
        .await;
    match result {
        Ok(resp) if resp.status().is_success() => Ok(()),
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Bindings API returned {status} for {proxy_id}: {body}"))
        }
        Err(e) => Err(format!("Bindings API request failed for {proxy_id}: {e}")),
    }
}
```

重构 `create_binding` 使用 `post_binding`：

```rust
pub async fn create_binding(
    &mut self,
    proxy_id: &str,
    outbound_json: &serde_json::Value,
) -> Result<u16, String> {
    let port = self.port_pool
        .allocate()
        .ok_or_else(|| "No available ports in pool".to_string())?;
    match self.post_binding(proxy_id, port, outbound_json).await {
        Ok(()) => {
            tracing::debug!("Created binding {proxy_id} on port {port}");
            Ok(port)
        }
        Err(e) => {
            self.port_pool.free(port);
            Err(e)
        }
    }
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo check`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src/singbox/process.rs
git commit -m "feat(server): add allocate_specific and create_binding_on_port for stable port reuse"
```

---

## Task 3: Server 启动时复用旧端口

**Files:**
- Modify: `src/main.rs:55-91`（启动流程）

- [ ] **Step 1: 修改启动流程，读取旧端口并复用**

将 main.rs 中的启动绑定逻辑改为：

```rust
// Load proxy pool from database (includes saved local_port values)
let pool = ProxyPool::new();
pool.load_from_db(&db);

// Save old port assignments before clearing memory state
let old_ports: Vec<(String, u16)> = pool.get_valid_proxies()
    .iter()
    .filter_map(|p| p.local_port.map(|port| (p.id.clone(), port)))
    .collect();

// Clear memory state (sing-box starts fresh, no active bindings yet)
pool.clear_all_local_ports();
// Note: do NOT clear DB local_ports — they are the source for port restoration

// Initialize SingboxManager and start
let mut manager = SingboxManager::new(config.singbox.clone(), config.validation.batch_size as u16);
if let Err(e) = manager.start().await {
    tracing::warn!("Failed to start sing-box: {e}");
}

// Create bindings for valid proxies, reusing old port assignments
{
    let mut proxies = pool.get_valid_proxies();
    proxies.truncate(config.singbox.max_proxies);
    if !proxies.is_empty() {
        let old_port_map: std::collections::HashMap<&str, u16> = old_ports
            .iter()
            .map(|(id, port)| (id.as_str(), *port))
            .collect();

        let mut assignments = Vec::new();
        for p in &proxies {
            let result = if let Some(&old_port) = old_port_map.get(p.id.as_str()) {
                // Try to restore old port
                manager.create_binding_on_port(&p.id, old_port, &p.singbox_outbound).await
            } else {
                // New proxy, sequential allocation
                manager.create_binding(&p.id, &p.singbox_outbound).await
            };
            match result {
                Ok(port) => assignments.push((p.id.clone(), port)),
                Err(e) => tracing::warn!("Failed to create binding for {}: {e}", p.id),
            }
        }

        for (id, port) in &assignments {
            pool.set_local_port(id, *port);
            db.update_proxy_local_port(id, *port as i32).ok();
        }
        tracing::info!(
            "Created {} initial bindings ({} restored, {} new)",
            assignments.len(),
            assignments.iter().filter(|(id, port)| old_port_map.get(id.as_str()) == Some(port)).count(),
            assignments.iter().filter(|(id, port)| old_port_map.get(id.as_str()) != Some(port)).count(),
        );
    }
}

// Now clear any DB ports for proxies that didn't get a binding
// (invalid proxies, proxies beyond max_proxies limit, etc.)
db.clear_all_proxy_local_ports_except(
    &pool.get_all()
        .iter()
        .filter(|p| p.local_port.is_some())
        .map(|p| p.id.as_str())
        .collect::<Vec<_>>()
).ok();
```

- [ ] **Step 2: 添加 `clear_all_proxy_local_ports_except` 到 DB**

在 `src/db.rs` 中添加：

```rust
pub fn clear_all_proxy_local_ports_except(&self, keep_ids: &[&str]) -> Result<(), rusqlite::Error> {
    let conn = self.conn.lock().unwrap();
    if keep_ids.is_empty() {
        conn.execute("UPDATE proxies SET local_port = NULL", [])?;
    } else {
        let placeholders: Vec<String> = keep_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "UPDATE proxies SET local_port = NULL WHERE id NOT IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = keep_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        conn.execute(&sql, params.as_slice())?;
    }
    Ok(())
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/db.rs
git commit -m "feat(server): reuse saved port assignments on restart for port stability"
```

---

## Task 4: Client 读取 SYNC_REMOTE_PORT 环境变量

**Files:**
- Modify: `sing-box-zenproxy/experimental/clashapi/server.go:106-112`
- Modify: `sing-box-zenproxy/experimental/clashapi/bindings.go:30-37`（BindingManager 结构体）

- [ ] **Step 1: 读取环境变量并传入 BindingManager**

在 `server.go` NewServer 函数中，读取 `SYNC_REMOTE_PORT`：

```go
// 在 portPool 初始化后添加
syncRemotePort := os.Getenv("SYNC_REMOTE_PORT") == "true"
s.bindingManager = newBindingManager(s, logFactory, proxyStore, portPool, syncRemotePort)
```

- [ ] **Step 2: 修改 BindingManager 结构体**

在 `bindings.go` 中：

```go
type BindingManager struct {
    server          *Server
    logger          log.ContextLogger
    bindings        map[string]*BindingInfo
    mu              sync.Mutex
    store           *ProxyStore
    portPool        *PortPool
    syncRemotePort  bool
}

func newBindingManager(server *Server, logFactory log.Factory, store *ProxyStore, portPool *PortPool, syncRemotePort bool) *BindingManager {
    return &BindingManager{
        server:         server,
        logger:         logFactory.NewLogger("bindings"),
        bindings:       make(map[string]*BindingInfo),
        store:          store,
        portPool:       portPool,
        syncRemotePort: syncRemotePort,
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cd sing-box-zenproxy && go build ./...`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add sing-box-zenproxy/experimental/clashapi/server.go sing-box-zenproxy/experimental/clashapi/bindings.go
git commit -m "feat(client): read SYNC_REMOTE_PORT env var"
```

---

## Task 5: Client Fetch 支持 sync_remote_port

**Files:**
- Modify: `sing-box-zenproxy/experimental/clashapi/remote_fetch.go`

- [ ] **Step 1: 扩展 serverProxy 和 remoteFetchRequest**

```go
type serverProxy struct {
    ID        string          `json:"id"`
    Name      string          `json:"name"`
    Type      string          `json:"type"`
    Server    string          `json:"server"`
    Port      uint16          `json:"port"`
    LocalPort *uint16         `json:"local_port,omitempty"` // 新增：远端绑定端口
    Outbound  json.RawMessage `json:"outbound"`
    Quality   json.RawMessage `json:"quality,omitempty"`
}

type remoteFetchRequest struct {
    Server         string `json:"server"`
    APIKey         string `json:"api_key"`
    Count          int    `json:"count"`
    Country        string `json:"country"`
    ChatGPT        *bool  `json:"chatgpt"`
    Type           string `json:"type"`
    AutoBind       bool   `json:"auto_bind"`
    SyncRemotePort *bool  `json:"sync_remote_port,omitempty"` // 新增
}
```

- [ ] **Step 2: 扩展 StoredProxy 以携带远端端口**

在 `store.go` 的 StoredProxy 中添加字段（如果需要），或在 Fetch 流程中直接使用 serverProxy 的 LocalPort。

在 remoteFetch 流程中，存储 proxy 时记录远端端口号：

```go
for _, sp := range serverResp.Proxies {
    p := StoredProxy{
        ID:       sp.ID,
        Name:     sp.Name,
        Type:     sp.Type,
        Server:   sp.Server,
        Port:     sp.Port,
        Outbound: sp.Outbound,
        Source:   "server",
    }
    if sp.LocalPort != nil {
        p.RemotePort = *sp.LocalPort
    }
    proxies = append(proxies, p)
}
```

在 `store.go` 的 `StoredProxy` 中添加：

```go
RemotePort uint16 `json:"remote_port,omitempty"`
```

- [ ] **Step 3: 实现 sync auto_bind 逻辑**

替换 remoteFetch 中的 auto_bind 段：

```go
// Determine sync mode: request param > env var > false
useSyncPort := bm.syncRemotePort // env default
if req.SyncRemotePort != nil {
    useSyncPort = *req.SyncRemotePort
}

// Auto-bind if requested
bindCount := 0
var syncErrors []map[string]string
if req.AutoBind {
    for _, p := range added {
        if useSyncPort {
            // Sync mode: use remote port, no fallback
            if p.RemotePort == 0 {
                syncErrors = append(syncErrors, map[string]string{
                    "proxy_id": p.ID,
                    "error":    "remote proxy has no local_port",
                })
                bm.logger.Warn("sync-port: proxy ", p.Name, " has no remote port, skipping")
                continue
            }
            binding, err := bm.createBindingDirect(p, p.RemotePort)
            if err != nil {
                syncErrors = append(syncErrors, map[string]string{
                    "proxy_id":    p.ID,
                    "remote_port": fmt.Sprintf("%d", p.RemotePort),
                    "error":       err.Error(),
                })
                bm.logger.Warn("sync-port failed for ", p.Name, " port ", p.RemotePort, ": ", err)
            } else {
                bindCount++
                _ = binding
            }
        } else {
            // Normal mode: auto-allocate from PortPool
            if _, err := bm.createBindingForProxy(p); err != nil {
                bm.logger.Warn("auto-bind failed for ", p.Name, ": ", err)
            } else {
                bindCount++
            }
        }
    }
}

bm.logger.Info("fetched ", len(added), " proxies from server")
result := render.M{
    "added":   len(added),
    "message": fmt.Sprintf("Fetched %d proxies from server", len(added)),
}
if req.AutoBind {
    result["bound"] = bindCount
    if len(syncErrors) > 0 {
        result["sync_errors"] = syncErrors
    }
}
render.JSON(w, r, result)
```

- [ ] **Step 4: 实现 `createBindingDirect` 方法**

在 `bindings.go` 中添加：

```go
// createBindingDirect creates a binding on a specific port without using PortPool.
// Used in sync_remote_port mode.
func (bm *BindingManager) createBindingDirect(proxy StoredProxy, port uint16) (*BindingInfo, error) {
    binding, err := bm.createBindingInternal(proxy.ID, port, proxy.Outbound, proxy.ID)
    if err != nil {
        return nil, err
    }
    bm.store.SetLocalPort(proxy.ID, port)
    return binding, nil
}
```

- [ ] **Step 5: 验证编译**

Run: `cd sing-box-zenproxy && go build ./...`
Expected: 编译通过

- [ ] **Step 6: Commit**

```bash
git add sing-box-zenproxy/experimental/clashapi/
git commit -m "feat(client): implement sync_remote_port in fetch with no-fallback policy"
```

---

## Task 6: 文档更新与收尾

**Files:**
- Modify: `docs/INTENT.md`（确认 §2 增量意图标记为已实现）
- Modify: `docs/SPEC.md`（更新端口约定表，添加 SYNC_REMOTE_PORT 说明）

- [ ] **Step 1: 更新 SPEC.md 端口约定表**

在端口约定表后添加：

```markdown
### 端口同步模式

客户端支持通过环境变量 `SYNC_REMOTE_PORT=true` 或 Fetch API 参数 `sync_remote_port` 开启端口同步模式。
开启后，客户端 Fetch 时将使用远端代理的端口号创建本地绑定，`PROXY_PORT_START/END` 失效。
端口冲突时报错跳过，不回退到自动分配。
```

- [ ] **Step 2: Commit**

```bash
git add docs/
git commit -m "docs: update SPEC for sync_remote_port feature"
```

---

## 前置已完成项（本次讨论中已修改）

以下修改已在讨论阶段完成，不属于 plan 执行范围，但记录在此以保持 traceability：

- [x] Docker compose 切换为 `network_mode: host`（4 个文件）
- [x] 删除 `.env` 文件
- [x] INTENT.md 添加 host 网络模式设计决策
- [x] INTENT.md 修正部署拓扑（VPS 不需要 Client）
- [x] WORKFLOW.md 修正使用态描述
- [x] SPEC.md 移除 `.env`，添加网络模式说明
