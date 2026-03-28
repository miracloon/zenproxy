# v0.36 Design — 端口稳定性、代理启用/禁用重构、订阅防呆、UI 修缮

> Audience: AI / Dev
> Status: Locked
> 本文记录 v0.36 设计讨论最终共识，作为 plan 阶段的输入。

---

## 1. 版本目标

v0.36 围绕两条主线展开：

1. **端口稳定性与代理生命周期重构** — 将端口分配从"验证状态驱动"改为"启用/禁用驱动"，彻底解耦验证与端口管理，确保下游服务 IP 稳定。
2. **UI 质量提升** — 修复若干显示 bug，统一弹窗风格，补全缺失的交互功能。

---

## 2. 核心架构变更：端口分配与代理状态解耦

### 2.1 当前问题

- `validate_all` 调用 `sync_proxy_bindings(Validation)` 和 `sync_proxy_bindings(Normal)`，验证过程本身会重新分配端口。
- 代理的 `status`（Valid/Invalid/Untested）同时决定了端口分配优先级和显示状态，职责耦合。
- 用户观察到"验证全部"后 Invalid 代理未被重新验证，因为当前逻辑只重置 `Valid + error_count > 0` 的代理为 Untested，已 Invalid 的代理永远不会被重新验证。
- 端口在验证过程中可能发生变动，影响下游服务稳定性。

### 2.2 新模型

**两个正交维度：**

| 维度 | 值 | 职责 |
|------|-----|------|
| **验证状态** (status) | Valid / Invalid / Untested | 纯显示，标记连通性结果 |
| **启用状态** (enabled) | 启用 / 禁用 | **唯一**决定是否分配服务端口 |

**核心规则：**
- 启用的代理 → 分配服务端口（无论 Valid 还是 Invalid）
- 禁用的代理 → 不分配服务端口
- 验证操作 → 只改 status，不触发 `sync_proxy_bindings`

### 2.3 `sync_proxy_bindings` 简化

当前 `SyncMode` 有三种模式（Normal / Validation / QualityCheck），按 status 分桶排序。

**新逻辑：**
- 移除 `SyncMode` 枚举
- 唯一判据：`is_disabled == false` → 分配端口
- 启用代理数 ≤ `max_proxies` 时，全部分配
- 不再按 Valid/Invalid/Untested 分桶排序

**端口记忆预留（Method A）：** 在分配新端口前，sync 函数持有 `singbox.lock()` 时，先从 DB 获取所有处于记忆期的禁用代理端口号，在 PortPool 中预占（`allocate_specific`），防止被分配给其他代理。分配逻辑完成后，释放预占但未实际使用的端口。

调用时机缩减为：
- 代理启用/禁用切换时
- 订阅刷新（新增/删除代理）后
- 服务启动时

**不再在 `validate_all` 和 `check_all` 中调用 `sync_proxy_bindings`。**

### 2.4 `validate_all` 重构

**新逻辑 — 两阶段验证：**

**阶段一：验证启用代理**
1. 收集所有**启用且有端口**的代理（无论当前 status）
2. 通过现有端口并发验证（复用 `validate_batch`）
3. 更新 status（Valid/Invalid）
4. 不动端口

**阶段二：验证禁用代理（临时端口）**
1. 收集所有**禁用**的代理
2. 按 `batch_size` 分批：
   a. 对每个代理：检查 DB 中是否有记忆端口
      - 有记忆端口 → 用记忆端口创建临时 sing-box binding
      - 无记忆端口 → 从 PortPool 分配临时端口创建 binding
   b. 通过临时端口并发验证
   c. 更新 status（Valid/Invalid）
   d. 销毁临时 binding，释放 PortPool 端口
   e. **不写 DB `local_port`** — 临时端口不产生记忆
3. 进入下一批，直到所有禁用代理验证完毕

**两种端口生命线：**

| 端口类型 | 触发 | 持久性 | 记忆 | 写入 DB `local_port` |
|----------|------|--------|------|---------------------|
| **服务端口** | 用户启用代理 | 持久存在直到禁用 | ✅ 有记忆（可配置时长） | ✅ 是 |
| **临时端口** | 验证禁用代理 | 验证完立即释放 | ❌ 不记忆 | ❌ 否 |

### 2.5 `validate_disabled`（独立操作）

新增独立函数，逻辑等同于 `validate_all` 的阶段二。只验证禁用代理，使用临时端口。

### 2.6 `check_all` 范围收窄

质检只对**启用 + 状态为 Valid + 有端口**的代理执行。不为质检分配临时端口，不触发端口重分配。

---

## 3. 端口记忆

### 3.1 场景

代理禁用时释放 sing-box binding（不占资源），但 DB 中保留端口号。再次启用时优先恢复原端口。

### 3.2 机制

- 禁用时：`disabled_at` 记录时间戳，sing-box binding 释放，`local_port` 保留在 DB 中
- 启用时：如果原端口号仍在 DB 中且未被占用 → 恢复原端口
- 端口预留：`sync_proxy_bindings` 中使用 Method A（见 §2.3），在 PortPool 中预占记忆期端口
- 删除代理时：端口号立即释放

### 3.3 记忆适用范围

- **服务端口**（用户主动启用的代理）→ 产生记忆
- **临时端口**（验证禁用代理时分配的）→ **不产生记忆**，验证完立即释放
- 验证禁用代理时，如果该代理有记忆端口 → 优先复用记忆端口做临时 binding

### 3.4 记忆过期（可配置）

- 设置项：`port_retention_hours`（默认 24，设为 0 表示禁用时立即清除端口号）
- 定时清理：扫描 `is_disabled = true AND local_port IS NOT NULL AND now - disabled_at > retention` → 清除 `local_port`
- 使用绝对时间戳（wall clock），容器暂停/停止/重建自然兼容

---

## 4. 订阅刷新防呆

### 4.1 当前行为

`refresh_subscription_core` 中已有部分防呆：fetch 失败或 parse 结果为 0 时保留旧代理。但这是隐式行为，未在设计上明确。

### 4.2 新设计：订阅层保护

**拉取失败** → 保留全部现有代理，不做任何变动，记录警告日志。

失败的判定：
- HTTP 错误 / 超时 / DNS 失败
- 解析后代理数为 0（视为异常结果）

**拉取成功**（≥1 个代理） → 正常增删改：
- 匹配到的代理（按 `(server, port, proxy_type)` 三元组）→ 原地更新信息，**启用状态不变**
- 消失的代理 → 直接删除（释放端口号）
- 新增的代理 → **默认启用**（已有订阅的刷新，新增节点直接启用并分配端口）

### 4.3 新增订阅

首次拉取 → 全部代理**默认禁用**。后台自动触发 `validate_all`，其阶段二会验证这些禁用代理。用户看到验证结果后一键启用有效代理。

API 响应中包含 `notice` 字段提醒用户：新增代理默认禁用，请验证后启用。

### 4.4 订阅删除

删除订阅 → 删除该订阅下全部代理（释放端口号）。

### 4.5 编辑订阅

编辑（改名、改 URL）只保存变更，**不触发刷新**。下次手动或自动刷新时使用新 URL。这和当前行为一致。

---

## 5. 前端批量操作按钮

在管理后台代理列表区域新增：

| 按钮 | 动作 |
|------|------|
| 一键启用有效代理 | 将所有 `status=Valid AND is_disabled=true` 的代理设为启用 |
| 一键禁用无效代理 | 将所有 `status=Invalid AND is_disabled=false` 的代理设为禁用 |
| 验证未启用代理 | 对所有禁用的代理进行临时端口验证，更新 status |

后续版本可扩展：多选、筛选条件、批量启用/禁用选中节点。v0.36 仅实现上述三个按钮。

---

## 6. 端口信息显示

### 6.1 当前状态

- `local_port` 在 API JSON 中返回，但未在任何页面渲染
- 仅显示远端 `server:port`

### 6.2 变更

在仪表盘（user.html）和管理后台（admin.html）的代理表格中新增"本地端口"列，显示 `local_port`。禁用或未分配的代理显示为空或 `—`。

---

## 7. UI 修缮

### 7.1 Modal 弹窗替换 prompt()

**问题**：管理后台"改名"和"重置密码"使用浏览器原生 `prompt()`，风格简陋。

**方案**：实现统一的 modal 组件（暗色主题、圆角、与现有 UI 一致），替换所有 `prompt()` 调用。该 modal 同时服务于：
- 管理后台：改名、重置密码
- 用户仪表盘：修改密码（见 7.2）

### 7.2 用户仪表盘修改密码

**问题**：
- `/api/auth/me` 返回的 JSON 缺少 `auth_source` 字段，导致前端无法判断用户登录方式
- 默认密码警告 banner 因此永远不显示

**方案**：
1. 修复 `/api/auth/me`，补充 `auth_source` 字段
2. 右上角用户名区域改为可点击，弹出下拉菜单
3. 仅对 `auth_source === 'password'` 的用户显示"修改密码"选项
4. 点击后弹出 modal（需输入旧密码 + 新密码 + 确认新密码）
5. 新增 API：`PUT /api/auth/password`（需验证旧密码，session 认证）
6. OAuth 用户不显示此选项（OAuth 用户无密码）

### 7.3 权限收紧：改名/重置密码仅超级管理员可操作

**问题**：当前 `PUT /api/admin/users/:id/username` 和 `PUT /api/admin/users/:id/password` 未做角色检查，任何 admin 都可调用。

**方案**：
- 后端：在 handler 中检查 `current_user.role == "super_admin"`，否则返回 403
- 前端：非 super_admin 登录时隐藏"改名"和"重置密码"按钮

### 7.4 OAuth 勾选框 UI 修复

**问题**：`<label>` 中使用了 Unicode 字符 `☑`，真正的 `<input type="checkbox">` 在下一行，视觉上出现两个勾选框。

**涉及位置**：
- "启用 Linux.do OAuth 登录"
- "允许用户注册"

**修复**：将 checkbox 移入 label，去掉 Unicode 字符：
```html
<!-- 修复前 -->
<label>☑ 启用 Linux.do OAuth 登录</label>
<input type="checkbox" id="set-linuxdo-enabled">

<!-- 修复后 -->
<label><input type="checkbox" id="set-linuxdo-enabled"> 启用 Linux.do OAuth 登录</label>
```

### 7.5 LinuxDo 信任等级范围修正

**问题**：当前 `max="4"`，提示"0~4"。LinuxDo 信任等级实际为 0~3（Basic / Member / Regular / Leader）。

**修复**：`max="3"`，提示改为"0~3"。

---

## 8. Favicon 图标端点

### 8.1 需求

支持浏览器收藏、收藏服务（如 Hoarder、Raindrop 等）自动获取图标。

### 8.2 方案

- 新增路由：`GET /favicon.ico`、`GET /icon.png`
- 文件位置：`data/favicon.ico` 和 `data/icon.png`（Docker volume 挂载路径）
- 如果文件存在 → 返回文件内容（正确 Content-Type）
- 如果文件不存在 → 返回 404（不内置默认图标）
- 所有 HTML 页面 `<head>` 中添加：
  ```html
  <link rel="icon" href="/favicon.ico" type="image/x-icon">
  <link rel="icon" href="/icon.png" type="image/png">
  <link rel="apple-touch-icon" href="/icon.png">
  ```
- 首页（用户仪表盘）可在标题旁显示该图标（如果存在）

---

## 9. 定时清理

统一的清理循环（建议每小时执行一次）：

```
扫描禁用代理:
  WHERE is_disabled = true
    AND local_port IS NOT NULL
    AND disabled_at IS NOT NULL
    AND now - disabled_at > port_retention_hours
  → 清除 local_port（代理本身保留）
```

v0.36 仅有此一项清理任务。结构上预留扩展能力（未来可在同一循环中添加其他清理逻辑）。

---

## 10. 并发安全

### 10.1 现有机制

| 锁 | 类型 | 保护对象 |
|---|------|---------|
| `state.singbox` | `Arc<Mutex<SingboxManager>>` | sing-box API 调用 + PortPool 分配/释放 |
| `state.validation_lock` | `Mutex<()>` | 序列化 `validate_all` / `check_all` / `validate_disabled` |
| `state.pool` | `DashMap`（lock-free concurrent map） | PoolProxy 读写 |

### 10.2 新增保护

**Toggle 操作与验证互斥：** `toggle_proxy` handler 中，如果 `validation_lock` 正在被持有，返回友好提示（"验证进行中，请稍后操作"），不阻塞等待。使用 `try_lock()` 实现。

**前端按钮互斥：** 验证进行中时，前端禁用所有"启用/禁用"toggle 按钮和其他验证按钮，防止用户重复操作。扩展已有的 `isValidating` 状态变量。

---

## 11. 新增 / 变更的设置项

| 设置 key | 类型 | 默认值 | 说明 |
|-----------|------|--------|------|
| `port_retention_hours` | integer | 24 | 禁用代理端口记忆时长（小时），0 = 禁用时立即清除 |

---

## 12. 新增 / 变更的 API

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| `PUT` | `/api/auth/password` | 用户自行修改密码（需旧密码） | session |
| `POST` | `/api/admin/proxies/enable-valid` | 一键启用所有有效代理 | admin |
| `POST` | `/api/admin/proxies/disable-invalid` | 一键禁用所有无效代理 | admin |
| `POST` | `/api/admin/proxies/validate-disabled` | 验证所有未启用代理 | admin |
| `GET` | `/favicon.ico` | Favicon | 无 |
| `GET` | `/icon.png` | PNG 图标 | 无 |

已有 API 变更：
| 方法 | 路径 | 变更 |
|------|------|------|
| `GET` | `/api/auth/me` | 返回值新增 `auth_source` 字段 |
| `PUT` | `/api/admin/users/:id/username` | 新增 super_admin 权限检查 |
| `PUT` | `/api/admin/users/:id/password` | 新增 super_admin 权限检查 |
| `POST` | `/api/admin/subscriptions` | 新订阅代理默认禁用，响应新增 `notice` 字段 |
| `POST` | `/api/admin/proxies/:id/toggle` | 新增 validation_lock try_lock 检查 |

---

## 13. 设计边界（不做的事）

- 不引入前端框架，继续 inline HTML + JS + CSS
- 不拆分 `db.rs`
- 不做多选/筛选批量操作（v0.36 仅一键启用有效 / 一键禁用无效 / 验证未启用）
- 不做"端口记忆超时自动删除代理"（仅清除端口号）
- 不做"冻结"状态（订阅刷新防呆在订阅层处理，不在代理层）
- 不改 Fetch/Relay API 认证方式
- 不改客户端（sing-box-zenproxy）代码 — 服务端变更对客户端透明

---

## 14. 全景状态机

```
                    ┌─────────────────────────────┐
                    │   新增订阅（首次拉取）         │
                    └──────────────┬──────────────┘
                                   ▼
                           ┌──────────────┐
                      ┌───▶│   disabled    │◀── 用户手动禁用
                      │    │  (默认状态)    │    (记录 disabled_at,
                      │    │              │     释放 binding,
                      │    └──────┬───────┘     DB 保留端口号)
                      │           │
                      │     用户启用 / 一键启用有效
                      │           │
                      │           ▼
                      │    ┌──────────────┐
                      │    │   enabled     │─── 分配服务端口，提供服务
   用户手动禁用 ◀─────┼────│              │    验证只改 valid/invalid
                      │    └──────────────┘
                      │
                      │  端口记忆超时（仅 disabled 状态）
                      └── 清除 DB 中 local_port
                           代理本身保留
                          （再启用时重新分配端口）

  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─

  验证流程（validate_all / validate_disabled）:
  
    启用代理（有端口）──▶ 通过现有端口验证 ──▶ 更新 status
    
    禁用代理 ──▶ 分配临时端口 ──▶ 验证 ──▶ 销毁临时端口
                 (有记忆端口则复用)        (不写 DB, 不记忆)
                 
  ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─

                    ┌─────────────────────────────┐
                    │   已有订阅刷新                 │
                    └──────────────┬──────────────┘
                                   ▼
                    拉取失败? ──是──▶ 不动，记录警告
                       │
                      否
                       ▼
                    匹配的代理 → 更新信息，状态不变
                    消失的代理 → 删除
                    新增的代理 → 默认启用
```
