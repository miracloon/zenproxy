# v0.39 Design — SOCKS 订阅认证解析修复与 IPv6 支持补齐

> Audience: AI / Dev
> Status: Locked
> 本文记录 v0.39 设计讨论最终共识，作为 plan 阶段输入。

---

## 1. 版本目标

v0.39 聚焦两个已经由真实使用场景确认的问题：

1. 修复订阅导入路径下，SOCKS URI 认证信息解析与逐个添加不一致的问题，保证相同节点经不同导入路径后落库结果一致
2. 将 IPv6 支持补齐到 **1 档 + 2 档**
   - IPv6 上游节点可导入、可验证、可质检
   - IPv4 接入但 IPv6 出口的节点，其最终出口可被质量检测识别
   - API / UI 能表达并筛选 IPv4 / IPv6 出口差异
   - 明确 **不做 3 档**：不改本地 listener 到 `::1` / 双栈监听

---

## 2. 已确认问题事实

### 2.1 SOCKS 订阅导入问题

已确认的用户场景如下：

- 同类 SOCKS 节点逐个添加时可正常连通
- 通过订阅 URL + `auto` 导入后，连通性测试失败
- 数据库中同类节点的 `config_json` 已证明订阅导入后的 `username/password` 被错误解析
- 根因不是 Tailscale 本身，也不是 SOCKS 协议本身，而是订阅解析路径未兼容编码后的 `userinfo`

一个已确认的错误落库结果示例：

```json
{"password":"","server":"100.99.99.1","server_port":20001,"type":"socks","username":"cnk6NjIxMzI%3D","version":"5"}
```

对应的正确目标结果应为：

```json
{"password":"62132","server":"100.99.99.1","server_port":20000,"type":"socks","username":"ry","version":"5"}
```

因此，v0.39 需要把“编码后的 `userinfo`”视为正式兼容范围，而不是偶然输入。

### 2.2 IPv6 支持边界

本版本对 IPv6 的目标边界已经明确：

- **支持**：
  - IPv6 上游节点导入
  - IPv6 节点连通性验证
  - IPv6 最终出口识别
  - API / UI 展示与筛选 IPv4 / IPv6
- **不支持**：
  - 本地 `::1` listener
  - 本地双栈 listener
  - 下游通过 IPv6 loopback 接入代理池

这意味着以下场景应纳入 v0.39 的正式支持：

1. `A -> B` 走 IPv4，`B -> C` 走 IPv6，`A` 仍使用 `127.0.0.1:600xx`
2. `A -> B` 走 IPv4，`B -> C` 走 IPv4，但 `C -> 目标网站` 的最终出口始终是 IPv6

---

## 3. T01 设计：SOCKS 订阅认证解析修复

### 3.1 目标边界

T01 只解决“同一条 SOCKS 节点在不同导入路径下落库结果不一致”的问题，不扩展成通用 parser 重构。

修复范围限定为：

- Rust Server 的订阅解析器
- Go Client 的订阅解析器（保持 fork 两端行为一致）
- `auto` / `v2ray` / `socks5` 入口下的 SOCKS / HTTP 类 URI 的 `userinfo`

### 3.2 最终兼容规则

对于 `socks://`、`socks5://`、`socks4://`、`http://`、`https://` 这类带 `userinfo` 的 URI，解析顺序固定为：

1. 先按当前明文规则尝试解析 `user:pass`
2. 若 `userinfo` 中不存在明文 `:`，先做 percent-decode
3. 对 percent-decode 后的结果再次检查是否包含 `:`
4. 若仍不包含 `:`，再尝试 base64 / base64url 解码
5. 若解码结果包含 `user:pass`，则以其作为最终认证信息
6. 若以上均失败，则保持“仅用户名、无密码”的现有兼容语义

### 3.3 一致性要求

无论节点通过哪条入口导入，只要表达的是同一组认证信息，最终落库的 outbound 关键字段都应一致，至少包括：

- `type`
- `server`
- `server_port`
- `version`
- `username`
- `password`

### 3.4 Rust / Go 一致性

由于本项目存在两套 parser：

- Rust Server：`src/parser/`
- Go Client：`sing-box-zenproxy/experimental/clashapi/parser/`

T01 必须同步修复两端，不允许只修一端而保留行为分叉。

### 3.5 不做事项

- 不重写整个 `parse_subscription_auto()`
- 不引入新的 URI 解析第三方库
- 不顺手扩展到和本问题无关的其它协议重构

---

## 4. T02 设计：IPv6 支持补齐（1 档 + 2 档）

### 4.1 目标边界

T02 只覆盖“IPv6 上游 / IPv6 最终出口”的正确处理，不覆盖“本地入口双栈”。

本任务要解决的是：

- IPv6 节点本身能正确解析并落库
- IPv6 节点连通性验证可正常工作
- 节点入口是 IPv4、但最终出口是 IPv6 时，质量检测能识别其最终出口家族
- API / UI 能把 IPv4 / IPv6 作为显式质量维度展示与筛选

### 4.2 分层目标

#### 解析层

- Rust parser 正确处理 IPv6 字面量
- Go parser 正确处理 IPv6 字面量
- 纯文本 parser 对 IPv6 形式不再误拆
- Clash YAML 中 IPv6 `server` 保持可用

#### 验证层

- 不更改本地 listener
- 继续通过 `127.0.0.1:port` 发起验证
- 验证只关心“代理链是否可达”，不关心本地入口地址族

#### 质量层

- 为质量结果增加显式 `ip_family`
- `ip_family` 以**实际探测到的出口 IP** 为准，而不是以上游节点配置为准
- 当节点入口是 IPv4、但最终出口是 IPv6 时，仍应记录为 `ipv6`
- 对第三方探测源做兼容与降级处理：只要已经拿到出口 IP，就不应把“IP 家族识别缺口”误判成“代理不可达”

#### 展示层

- 管理后台显示 `IPv4` / `IPv6`
- 用户页显示 `IPv4` / `IPv6`
- 支持按 IP family 过滤
- API 输出中补齐 `ip_family`

### 4.3 `ip_family` 的建模策略

v0.39 优先采用**派生字段**策略，而不是强制新增数据库 schema：

- 优先从质检得到的 `ip_address` 派生 `ip_family`
- `ip_family` 在内存对象与 API/UI 中作为显式字段存在
- 若当前数据源已经能提供 `ip_address`，则无需额外迁移数据库表

这样可以降低迁移面，并减少对现有 `proxy_quality` 表的破坏性。

若实现阶段发现仅靠派生字段无法稳定支撑筛选与回显，才允许升级为数据库持久化字段。

### 4.4 质量检测的兼容策略

质量检测仍沿用现有主流程：

- 继续复用现有通过代理访问探测站点的机制
- 继续保留 `ip-api.com` 与 `ipinfo.io` 的角色分工
- 允许补充一个轻量的“当前出口 IP 探针”作为保底数据源，只负责给出出口 IP 与家族，不承担风险评分职责

最终策略要求：

1. 风险 / hosting / residential 等增强信息仍可来自现有提供方
2. `ip_family` 必须以最终探测到的出口 IP 为准
3. 若增强信息缺失但出口 IP 已识别，不应直接判定为“全部质检失败”

### 4.5 不做事项

- 不做本地 `::1` listener
- 不做 bind-address 双栈配置
- 不改 sing-box 核心网络行为
- 不把 IPv6 话题扩展成“所有外部探测服务全面替换”

---

## 5. 任务关系

v0.39 建议按以下顺序推进：

```text
T01 SOCKS 订阅认证解析修复（Rust）
  └─→ T02 SOCKS 订阅认证解析修复（Go）
        └─→ T03 IPv6 解析层补齐（Rust）
              └─→ T04 IPv6 解析层补齐（Go）
                    └─→ T05 IPv6 质量检测与 ip_family 建模（Rust）
                          └─→ T06 API / UI / README 补齐（Rust + HTML + Docs）
```

原因：

1. 先用 T01 锁定 Rust Server 的 parser 行为，再用 T02 同步 Go Client，执行边界和提交边界都更清晰
2. T03 / T04 继续沿同样节奏补齐 Rust / Go 的 IPv6 解析层，避免再次把两端改动揉在一起
3. T05 不要求大改架构，只需在现有质检链路上补齐 IPv6 family 识别与降级策略
4. `ip_family` 一旦进入质量对象，T06 的 API / UI 补齐才有意义

---

## 6. 风险、复杂度与兼容策略

### 6.1 T01 风险评估

- **复杂度**：低到中
- **风险**：中
- **破坏性**：低

风险点主要在于：

- `userinfo` 解码顺序如果处理不当，可能误伤现有“仅用户名”语义
- Rust / Go 两端 parser 若未同步，仍会保留行为分叉

兼容策略：

- 先保留明文解析路径，再增加 percent/base64 兼容分支
- 用回归测试锁定“明文输入不能被修坏”

### 6.2 T02 风险评估

- **复杂度**：中到中高
- **风险**：中
- **破坏性**：低到中

风险点主要在于：

- 纯文本 parser 对 IPv6 字面量的切分边界
- 质量检测依赖第三方服务，IPv6 场景下可能出现“出口可达但增强信息缺失”
- API / UI 加入 `ip_family` 后，需要保证旧字段兼容不破坏现有调用方

兼容策略：

- 优先使用派生字段，减少 DB 迁移
- 质量层先把“出口 IP / family”识别稳定，再谈更细的增强属性
- 前端展示增加 `ip_family` 时保持旧字段结构不变，只做新增

---

## 7. 版本边界总结

v0.39 的正式边界如下：

- **必须完成**
  - SOCKS 订阅认证解析修复
  - IPv6 节点导入 / 验证 / 质量检测补齐到 1 档 + 2 档
  - API / UI 对 IPv4 / IPv6 家族的表达与筛选
- **允许顺手补齐**
  - parser 回归测试
  - 质量对象附带字段
  - README 说明
  - Rust / Go parser 一致性修复
- **明确不做**
  - 本地 `::1` / 双栈 listener
  - sing-box 核心改造
  - 大规模 parser 架构重写
