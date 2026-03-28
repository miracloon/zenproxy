# v0.37 Design — UI 精细化控制、多选批量操作、状态模型解耦

> Audience: AI / Dev
> Status: Locked
> 本文记录 v0.37 设计讨论最终共识，作为 plan 阶段的输入。

---

## 1. 版本目标

v0.37 围绕管理后台 UI 精细化控制展开：

1. **状态模型解耦** — 将 `ProxyStatus::Disabled` 移除，验证状态（valid/invalid/untested）与启用状态（enabled/disabled）完全正交
2. **操作列固定布局** — 4 个固定按钮（启用/禁用、连通测试、质量检测、删除），不因状态隐藏
3. **多选功能** — checkbox 多选 + 全选/选本页，支持跨页保留选中
4. **批量操作扩展** — 通用 batch API，选中操作 + 全局操作分组
5. **操作区分组 UI** — 四分区布局（选中操作、验证与质检、快捷、清理）
6. **分页大小可配置** — 30/50/100 三档
7. **质检安全网** — 质检四路全败自动标记 Invalid

---

## 2. 状态模型解耦

### 2.1 双维度正交模型

| 维度 | 字段 | 可能值 | 含义 |
|------|------|--------|------|
| 验证状态 | `status` | `Valid` / `Invalid` / `Untested` | 连通性结果 |
| 启用状态 | `is_disabled` | `true` / `false` | 是否参与端口绑定 |

**`ProxyStatus` 枚举移除 `Disabled` 变体**，只保留 `Untested | Valid | Invalid`。

### 2.2 行为变更

- `set_disabled()` 只修改 `is_disabled`，**不动 `status`**
- `load_from_db()` 不以 `is_disabled` 覆盖 `status`
- 禁用代理保留其验证状态（禁用前是 Valid 就仍然显示 Valid）
- 重新启用后**不重置为 Untested**

### 2.3 前端显示

```
启用 + 有效:    [有效]
启用 + 无效:    [无效]
启用 + 待测试:  [待测试]
禁用 + 有效:    [有效] [已禁用]    ← 双 badge
禁用 + 无效:    [无效] [已禁用]
禁用 + 待测试:  [待测试] [已禁用]
```

### 2.4 筛选栏

拆为两个独立下拉框：
- 验证状态：全部 / 有效 / 无效 / 待测试
- 启用状态：全部 / 已启用 / 已禁用

---

## 3. 操作门槛

### 3.1 最终规则

| 操作 | 门槛 | 说明 |
|------|------|------|
| 连通测试 | 无 | 任何代理随时可测试（禁用代理使用临时端口+端口记忆） |
| 质量检测 | `status == valid` | 通过连通测试是前提。无关 `is_disabled` |
| 启用/禁用 | 无 | 随时可切换 |
| 删除 | 无 | 二次确认 |

### 3.2 质检安全网

质检 `check_single()` 返回结果后，如果四路探测全空（`ip_address == None && !google && !chatgpt`），判定代理实际不可达，自动标记 `status = Invalid`。不保存空质检数据。

### 3.3 单个操作门槛在 UI 的体现

- 连通测试按钮：始终可点击
- 质量检测按钮：`status != valid` 时 `disabled`，不可点击
- 启用/禁用按钮：始终可点击（验证进行中时 disabled）
- 删除按钮：始终可点击

---

## 4. 并发限制

### 4.1 前端硬编码

```
MAX_CONCURRENT_QUALITY  = 3   // 保护 ip-api.com 速率（45/分钟）
MAX_CONCURRENT_VALIDATE = 5   // 连通测试并发宽松
```

达到上限时，其他代理的对应按钮临时 disabled + toast 提示。

---

## 5. 多选功能

### 5.1 核心规则

- 筛选 ≠ 选中。筛选只缩小视觉范围
- 选中状态存 `Set<string>`（proxy ID），不依赖 DOM
- 跨页保留、跨筛选保留

### 5.2 全选交互

- "全选" checkbox = 当前筛选结果全部选中（跨页）
- "选本页" = 仅选中当前页
- 选中数量实时显示

---

## 6. 批量操作

### 6.1 通用 batch API

```
POST /api/admin/proxies/batch
Body: { "action": "enable" | "disable" | "validate" | "quality" | "delete", "ids": [...] }
Response: { "action": "...", "total": N, "processed": M, "skipped": K, "message": "..." }
```

### 6.2 防护矩阵

| 操作 | 过滤规则 | 跳过 |
|------|---------|------|
| enable | 仅 `is_disabled == true` | 已启用的 |
| disable | 仅 `is_disabled == false` | 已禁用的 |
| validate | 全执行 | 无 |
| quality | 仅 `status == valid` | 非 valid |
| delete | 全执行 | 无（二次确认） |

### 6.3 操作区四分组

```
① 选中操作（有选中时激活）
② 验证与质检（验证全部、验证未启用、验证无效、质检全部）
③ 快捷操作（一键启用有效、一键禁用无效）
④ 清理 / 危险操作（清理无效、清理三不通）
```

### 6.4 新增全局操作

| 操作 | API | 说明 |
|------|-----|------|
| 验证无效 | `POST /api/admin/proxies/validate-invalid` | 验证所有 `status == invalid` 的代理 |

"质检全部"扩展范围：从"启用+有效+有端口"扩展为"所有 `status == valid`"（包括禁用但有效的，使用临时端口+端口记忆）。

---

## 7. 分页

默认 50，可选 30 / 50 / 100。`localStorage` 记住偏好。

---

## 8. 端口记忆修复

### 8.1 现有问题

`validate_single_proxy()` 和 `quality_check_single_proxy()` 创建临时绑定时未查询 DB 端口记忆，直接调用 `create_binding()`。

### 8.2 修复

两个 handler 均需添加 DB 端口查询逻辑，有记忆端口时使用 `create_binding_on_port()`，与批量验证 `validate_disabled_proxies()` 一致。

---

## 9. 设计边界（不做的事）

- 不引入前端框架
- 不拆分 `db.rs`
- 不做"质检前自动验证"（门槛即为 valid，不隐式触发验证）
- 不做"全部"分页选项（性能风险）
- 不改客户端代码
- 不改 Fetch/Relay API
