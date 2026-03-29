# v0.39 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 SOCKS 订阅导入时的认证解析错误，并将 IPv6 支持补齐到“可导入、可验证、可质检、可展示/筛选”的 1 档 + 2 档范围。

**Architecture:** 先统一 Rust Server 与 Go Client 的 URI parser 行为，补齐编码 `userinfo` 与 IPv6 字面量解析；随后在 Rust Server 的质量检测链路中增加 `ip_family` 建模与 IPv6 兼容降级策略，并把该字段暴露给 Fetch / Relay / Admin / User 侧 API 与前端展示。整个版本明确不改本地 `127.0.0.1` listener。

**Tech Stack:** Rust/Axum（server API / parser / quality）, Go/sing-box Clash API parser（client fork）, SQLite（现有 quality 数据）, vanilla HTML/CSS/JS（admin / user UI）

---

## File Structure

### Modified Files

| File | Tasks | Changes |
| --- | --- | --- |
| `src/parser/v2ray.rs` | T01, T03 | Rust 侧 `userinfo` 解码兼容、IPv6 URI 解析测试与辅助函数 |
| `src/parser/plain.rs` | T03 | Rust 侧纯文本 IPv6 host:port / 带认证形式解析补齐 |
| `src/parser/clash.rs` | T03 | Rust 侧 Clash YAML 的 IPv6 server 测试与必要兼容 |
| `src/pool/manager.rs` | T05 | `ProxyQualityInfo` 增加 `ip_family`，`ProxyFilter` 增加 family 过滤 |
| `src/quality/checker.rs` | T05 | 质量检测补齐 IPv6 出口识别、降级策略与测试 |
| `src/api/fetch.rs` | T05, T06 | 暴露 `ip_family`，支持 family 过滤 |
| `src/api/client_fetch.rs` | T05, T06 | 返回 `ip_family`，支持 family 过滤与测试 |
| `src/api/relay.rs` | T05 | Relay 支持按 family 过滤 |
| `src/api/admin/proxies.rs` | T05 | 管理后台代理列表 JSON 输出 `ip_family` |
| `src/web/admin.html` | T06 | 管理后台展示 / 筛选 `IPv4` / `IPv6` |
| `src/web/user.html` | T06 | 用户页展示 / 筛选 `IPv4` / `IPv6` |
| `README.md` | T06 | 补充 SOCKS 编码认证兼容说明与 IPv6 支持边界 |
| `sing-box-zenproxy/experimental/clashapi/parser/v2ray.go` | T02, T04 | Go parser 同步修复编码 `userinfo` 与 IPv6 解析 |

### New Files

| File | Tasks | Purpose |
| --- | --- | --- |
| `sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go` | T02, T04 | 覆盖 Go parser 的 encoded `userinfo` 与 IPv6 行为 |

### No New Runtime Modules Expected

v0.39 以补齐现有 parser / quality / UI 能力为主，默认不新增新的运行时模块文件；Rust 侧优先在现有文件内补测试与辅助函数。

---

## Task Carrier Decision

v0.39 使用单一 `plan.md` 承载任务边界，不再拆分 `taskNN.md`。

原因：

1. 本次版本虽然跨 Rust / Go / HTML / Docs，但仍围绕两条清晰主线推进：`SOCKS parser 一致性` 与 `IPv6 质量链路补齐`
2. 任务间依赖明确，拆成多个 task 文件收益不高
3. exec 阶段仍需严格按本计划中的任务边界推进，并在每个任务单元完成后先验证、再 commit

---

## Task Dependency Graph

```text
T01 (SOCKS 订阅认证解析修复：Rust)
  └─→ T02 (SOCKS 订阅认证解析修复：Go)
        └─→ T03 (IPv6 解析层补齐：Rust)
              └─→ T04 (IPv6 解析层补齐：Go)
                    └─→ T05 (IPv6 质量检测与 ip_family 建模：Rust)
                          └─→ T06 (API / UI / README 补齐)
```

**Critical path:** T01 → T02 → T03 → T04 → T05 → T06

---

## Verification Strategy

1. Rust parser / quality 相关变更优先使用单元测试锁定行为，再运行 `cargo test`
2. Go parser 相关变更新增 `v2ray_test.go`，运行 `go test ./experimental/clashapi/parser`
3. API / UI 改动至少保证 `cargo test` 通过，并对 `admin.html` / `user.html` 做最小语法检查
4. 最终手测需覆盖：
   - 编码 `userinfo` 的 SOCKS 节点订阅导入
   - 逐个添加与订阅导入后的 `config_json` 对比
   - IPv6 节点或 IPv4 入口 / IPv6 出口节点的验证与质检
   - Admin / User 页的 `ip_family` 展示与筛选

---

## Detailed Tasks

### Task 01: SOCKS 订阅认证解析修复（Rust）

**Files:**
- Modify: `src/parser/v2ray.rs`

**What to do:**

- [ ] **Step 1: 先写 Rust 失败测试，锁定 encoded `userinfo` 的目标行为**

  在 `src/parser/v2ray.rs` 增加最小测试，至少覆盖：

  - `socks5://ry:62132@host:port#name` 明文输入保持现有结果
  - `userinfo` 先 percent-decode 后得到 `user:pass`
  - `userinfo` percent-decode 后再 base64 解码得到 `user:pass`
  - `userinfo` 无法解码时仍保留“仅用户名、无密码”的兼容语义
  - 相同语义的 URI 经不同编码形式解析后，最终 `username/password` 一致

- [ ] **Step 2: 运行 Rust 目标测试，确认先失败**

  Run: `cargo test parser:: -- --nocapture`

  Expected: 新增的 encoded `userinfo` 测试失败，失败原因应与当前 parser 未兼容编码认证直接相关。

- [ ] **Step 3: 在 Rust parser 中提炼共享 `userinfo` 解码辅助函数**

  在 `src/parser/v2ray.rs` 中提炼 helper，规则固定为：

  1. 先按明文 `user:pass` 尝试
  2. 再尝试 percent-decode
  3. 再尝试 base64 / base64url 解码
  4. 最终回退到“仅用户名”

  要求：

  - `parse_socks()` 使用该 helper
  - `parse_http_proxy()` 也复用该 helper，避免 Rust 内部行为继续分叉

- [ ] **Step 4: 重新跑 Rust 目标测试确认通过**

  Run:

  ```bash
  cargo test parser:: -- --nocapture
  ```

  Expected: Rust parser 新增测试全部通过。

- [ ] **Step 5: 运行 Rust 全量测试确认无回归**

  ```bash
  cargo test
  ```

  Expected: 通过。

- [ ] **Step 6: Commit**

  ```bash
  git add src/parser/v2ray.rs
  git commit -m "fix(server-parser): normalize encoded proxy credentials"
  ```

---

### Task 02: SOCKS 订阅认证解析修复（Go）

**Files:**
- Modify: `sing-box-zenproxy/experimental/clashapi/parser/v2ray.go`
- Create: `sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go`

**What to do:**

- [ ] **Step 1: 先写 Go 失败测试，锁定与 Rust 一致的行为**

  在 `sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go` 至少覆盖：

  - 明文 SOCKS 认证
  - encoded `userinfo`
  - HTTP/HTTPS URI 的同类 `userinfo`
  - 结果中的 `username/password` 与 Rust 预期一致

- [ ] **Step 2: 运行 Go 目标测试，确认先失败**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi/parser -run 'TestParse(Socks|HTTP)' -count=1
  ```

  Expected: Go parser 因尚未兼容 encoded `userinfo` 而失败。

- [ ] **Step 3: 在 Go parser 同步实现同一套 `userinfo` 解码规则**

  在 `sing-box-zenproxy/experimental/clashapi/parser/v2ray.go` 中：

  - 提炼共享 `userinfo` 解析 helper
  - `parseSocks()` 与 `parseHTTPProxy()` 共用
  - 解码顺序与 Rust 完全一致

- [ ] **Step 4: 重新跑 Go 目标测试确认通过**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi/parser -run 'TestParse(Socks|HTTP)' -count=1
  ```

  Expected: Go parser 测试全部通过，且行为与 Rust 一致。

- [ ] **Step 5: 运行 Go parser 全量测试确认无回归**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi/parser -count=1
  ```

  Expected: 通过。

- [ ] **Step 6: Commit**

  ```bash
  git add sing-box-zenproxy/experimental/clashapi/parser/v2ray.go \
          sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go
  git commit -m "fix(client-parser): normalize encoded proxy credentials"
  ```

---

### Task 03: IPv6 解析层补齐（Rust）

**Files:**
- Modify: `src/parser/v2ray.rs`
- Modify: `src/parser/plain.rs`
- Modify: `src/parser/clash.rs`

**What to do:**

- [ ] **Step 1: 先写 Rust 失败测试，锁定 IPv6 输入形式**

  至少覆盖：

  - `socks5://user:pass@[2001:db8::1]:1080#name`
  - `http://user:pass@[2001:db8::1]:8080`
  - 纯文本 `[2001:db8::1]:1080`
  - 纯文本 `user:pass@[2001:db8::1]:1080`
  - Clash YAML 中 `server: 2001:db8::1`、`port: 1080`

  目标是锁定：

  - host 不被误拆
  - `server_port` 正确
  - 认证字段与 IPv4 行为一致

- [ ] **Step 2: 运行 Rust 目标测试，确认先失败**

  Run: `cargo test parser:: -- --nocapture`

  Expected: 纯文本 IPv6 或边界输入至少有一项失败，证明当前支持不完整。

- [ ] **Step 3: 实现 Rust 侧 IPv6 解析补齐**

  要求：

  - `parse_host_port()` 继续承担带 `[]` 的 URI host 解析
  - `plain.rs` 不再简单依赖冒号数量，需显式兼容 bracketed IPv6
  - Clash YAML 若现有逻辑已支持，则仅补测试；若有边界缺口，再做最小修正

- [ ] **Step 4: 重新跑 Rust 目标测试确认通过**

  Run:

  ```bash
  cargo test parser:: -- --nocapture
  ```

  Expected: Rust 解析层新增测试全部通过。

- [ ] **Step 5: 运行 Rust 全量测试确认无回归**

  Run:

  ```bash
  cargo test
  ```

  Expected: 通过。

- [ ] **Step 6: Commit**

  ```bash
  git add src/parser/v2ray.rs src/parser/plain.rs src/parser/clash.rs
  git commit -m "feat(server-parser): support ipv6 proxy inputs"
  ```

---

### Task 04: IPv6 解析层补齐（Go）

**Files:**
- Modify: `sing-box-zenproxy/experimental/clashapi/parser/v2ray.go`
- Modify: `sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go`

**What to do:**

- [ ] **Step 1: 为 Go parser 写 IPv6 失败测试**

  在 `sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go` 增加：

  - SOCKS / HTTP URI 的 bracketed IPv6 host
  - `parseHostPort()` 对 IPv6 的行为

- [ ] **Step 2: 运行 Go 目标测试，确认先失败**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi/parser -run 'TestParse(IPv6|Socks|HTTP)' -count=1
  ```

  Expected: 至少一项 IPv6 边界测试失败。

- [ ] **Step 3: 实现 Go 侧 IPv6 解析补齐**

  要求：

  - `parseHostPort()` 的 bracketed IPv6 语义与 Rust 对齐
  - `parseSocks()` / `parseHTTPProxy()` 在 IPv6 host 下仍能正确处理 `userinfo`

- [ ] **Step 4: 重新跑 Go 目标测试确认通过**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi/parser -run 'TestParse(IPv6|Socks|HTTP)' -count=1
  ```

  Expected: Go 解析层新增测试全部通过。

- [ ] **Step 5: 运行 Go parser 全量测试确认无回归**

  Run:

  ```bash
  cd sing-box-zenproxy
  go test ./experimental/clashapi/parser -count=1
  ```

  Expected: 通过。

- [ ] **Step 6: Commit**

  ```bash
  git add sing-box-zenproxy/experimental/clashapi/parser/v2ray.go \
          sing-box-zenproxy/experimental/clashapi/parser/v2ray_test.go
  git commit -m "feat(client-parser): support ipv6 proxy inputs"
  ```

---

### Task 05: IPv6 质量检测与 `ip_family` 建模（Rust）

**Files:**
- Modify: `src/pool/manager.rs`
- Modify: `src/quality/checker.rs`
- Modify: `src/api/fetch.rs`
- Modify: `src/api/client_fetch.rs`
- Modify: `src/api/relay.rs`
- Modify: `src/api/admin/proxies.rs`

**What to do:**

- [ ] **Step 1: 先写失败测试，锁定 `ip_family` 派生规则**

  在 `src/quality/checker.rs` 或 `src/pool/manager.rs` 增加最小测试，至少覆盖：

  - `203.0.113.1` → `ipv4`
  - `2001:db8::1` → `ipv6`
  - 无 IP → `None`

- [ ] **Step 2: 为筛选逻辑写失败测试**

  在 `src/pool/manager.rs` 或 `src/api/client_fetch.rs` 增加测试，至少覆盖：

  - family = `ipv4` 时只返回 IPv4 质量结果
  - family = `ipv6` 时只返回 IPv6 质量结果
  - 未指定 family 时保持现有行为

- [ ] **Step 3: 运行 Rust 目标测试，确认先失败**

  Run:

  ```bash
  cargo test quality::checker pool::manager api::client_fetch -- --nocapture
  ```

  Expected: `ip_family` 相关测试因字段 / 过滤尚未存在而失败。

- [ ] **Step 4: 在质量对象中增加派生 `ip_family`**

  在 `src/pool/manager.rs`：

  - 为 `ProxyQualityInfo` 增加 `ip_family: Option<String>`
  - 从 `ip_address` 派生 `ipv4` / `ipv6`
  - 从 DB 加载旧质量数据时也进行同样派生

  要求：

  - 默认不改 `proxy_quality` 表 schema
  - 优先使用派生字段，减少迁移面

- [ ] **Step 5: 在质量检测流程中补齐 IPv6 出口识别与降级策略**

  在 `src/quality/checker.rs`：

  - 统一使用最终探测到的出口 IP 派生 `ip_family`
  - 若增强信息缺失但已拿到出口 IP，不应视为“all probes failed”
  - 允许引入一条轻量 current-IP 探针作为保底数据源，但只负责出口 IP / family，不承担风险评分职责

- [ ] **Step 6: 为过滤入口增加 family 维度**

  在以下文件中补 `ip_family` 过滤：

  - `src/pool/manager.rs` 的 `ProxyFilter`
  - `src/api/fetch.rs`
  - `src/api/client_fetch.rs`
  - `src/api/relay.rs`

  要求：

  - 字段命名统一为 `ip_family`
  - 可接受值为 `ipv4` / `ipv6`
  - 未提供时保持现有筛选语义

- [ ] **Step 7: 为 API 输出补齐 `ip_family`**

  在以下 JSON 输出中补字段：

  - `src/api/fetch.rs`
  - `src/api/client_fetch.rs`
  - `src/api/admin/proxies.rs`

  要求：

  - 旧字段全部保留
  - `quality` 内新增 `ip_family`

- [ ] **Step 8: 重新跑目标测试确认通过**

  Run:

  ```bash
  cargo test quality::checker pool::manager api::client_fetch -- --nocapture
  ```

  Expected: `ip_family` 派生与过滤相关测试通过。

- [ ] **Step 9: 运行 Rust 全量测试确认无回归**

  Run: `cargo test`

  Expected: 全部通过。

- [ ] **Step 10: Commit**

  ```bash
  git add src/pool/manager.rs \
          src/quality/checker.rs \
          src/api/fetch.rs \
          src/api/client_fetch.rs \
          src/api/relay.rs \
          src/api/admin/proxies.rs
  git commit -m "feat(quality): add ipv6 family awareness"
  ```

---

### Task 06: API / UI / README 补齐

**Files:**
- Modify: `src/web/admin.html`
- Modify: `src/web/user.html`
- Modify: `README.md`

**What to do:**

- [ ] **Step 1: 先列出文档与 UI 必须说清楚的行为**

  至少包括：

  - `ip_family` 的取值与含义
  - IPv4 / IPv6 筛选入口
  - “支持 IPv6 上游 / IPv6 出口，但本地 listener 仍为 `127.0.0.1`” 的边界
  - SOCKS 编码 `userinfo` 的兼容形式

- [ ] **Step 2: 更新 `src/web/admin.html`**

  补齐：

  - 代理表新增 `IPv4` / `IPv6` 展示
  - family 过滤项
  - 兼容旧数据（无 `ip_family` 时显示 `-`）

- [ ] **Step 3: 更新 `src/web/user.html`**

  补齐：

  - family 展示
  - family 过滤项
  - 与 `fetch` / `relay` 控件联动的 family 参数

- [ ] **Step 4: 更新 `README.md`**

  需要补齐：

  - SOCKS / HTTP URI 编码认证兼容说明
  - IPv6 支持边界
  - `ip_family` API 字段与筛选参数
  - 明确说明 v0.39 不提供 `::1` / 双栈本地 listener

- [ ] **Step 5: 做最小前端与文档验证**

  Run:

  ```bash
  cargo test
  ```

  并人工检查：

  - `admin.html` 中 family 展示 / 筛选项存在
  - `user.html` 中 family 展示 / 筛选项存在

- [ ] **Step 6: Commit**

  ```bash
  git add src/web/admin.html src/web/user.html README.md
  git commit -m "docs(ui): expose ipv6 family support"
  ```

---

## Final Verification Before Summary

在 exec 阶段完成全部任务后，进入 summary 前至少执行：

```bash
cargo test
cd sing-box-zenproxy && go test ./experimental/clashapi/parser -count=1
```

以及至少一次手测：

1. 导入一个带编码 `userinfo` 的 SOCKS 节点订阅
2. 导入同一节点的逐个添加版本
3. 对比数据库 `config_json`
4. 验证导入节点连通性恢复正常
5. 对一个 IPv6 节点，或一个 IPv4 接入 / IPv6 最终出口节点执行质量检测
6. 确认 API 与 UI 中可见 `ip_family`

只有这些验证结果明确后，才能进入 `summary.md`。
