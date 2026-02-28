# ZenProxy

ZenProxy 是一个代理池管理与转发服务，基于 Rust + Axum 构建，使用修改版 sing-box 作为代理后端。支持多协议代理订阅导入、自动验证与质检、OAuth 用户认证、以及 HTTP 请求转发（relay）。

## 架构概览

```
用户请求 → ZenProxy (Axum) → sing-box (动态 Bindings) → 代理服务器 → 目标网站
```

- **ZenProxy**：Web 服务，管理代理池、用户认证、请求转发
- **sing-box（修改版）**：代理后端，通过 REST API 动态管理代理绑定，无需重启

## 修改版 sing-box

ZenProxy 使用的 sing-box 在官方版本基础上新增了 **Bindings REST API**，支持运行时动态增删代理绑定：

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST /bindings` | 创建绑定 | Body: `{"tag": "proxy_id", "listen_port": 10002, "outbound": {...}}` → 201 |
| `DELETE /bindings/{tag}` | 删除绑定 | → 204 |
| `GET /bindings` | 列出所有绑定 | → 200 + JSON 数组 |

sing-box 启动时仅加载最小配置（Clash API + direct outbound），所有代理通过 API 动态添加。**永远不需要重启 sing-box 进程。**

### 编译 sing-box

```bash
# Linux (在 sing-box-dev-next 目录)
GOOS=linux GOARCH=amd64 CGO_ENABLED=0 go build -o sing-box -tags with_clash_api ./cmd/sing-box
```

sing-box 二进制文件查找优先级：
1. 与 zenproxy 同目录下的 `sing-box`（推荐）
2. 配置文件指定的 `binary_path`
3. 系统 PATH

## 支持的代理协议

- VMess
- VLESS
- Trojan
- Shadowsocks
- Hysteria2

## 支持的订阅格式

| 类型 | 说明 |
|------|------|
| `auto` | 自动检测（默认），依次尝试 Clash YAML → Base64 V2Ray → 原始 V2Ray URI |
| `clash` | Clash YAML 格式（`proxies:` 字段） |
| `v2ray` | V2Ray URI 格式，每行一个 `vmess://`、`vless://`、`trojan://` 等 |
| `base64` | Base64 编码的 V2Ray URI 列表 |

## 配置文件

ZenProxy 从 `config.toml` 读取配置（与可执行文件同目录），示例：

```toml
[server]
host = "0.0.0.0"
port = 3000
admin_password = "your-admin-password"    # 管理后台密码
min_trust_level = 1                       # OAuth 用户最低信任等级（Linux.do trust_level）

[oauth]
client_id = ""                            # Linux.do OAuth client ID
client_secret = ""                        # Linux.do OAuth client secret
redirect_uri = "https://your-domain.com/api/auth/callback"

[singbox]
binary_path = "/usr/local/bin/sing-box"   # sing-box 二进制路径（同目录优先）
config_path = "data/singbox-config.json"  # sing-box 运行配置路径（自动生成）
base_port = 10001                         # 代理端口起始值（分配范围: base_port+1 ~ base_port+max_proxies）
max_proxies = 300                         # 最大同时绑定代理数（默认 300）
api_port = 9090                           # sing-box Clash API 端口（默认 9090）
api_secret = ""                           # sing-box API 密钥（可选）

[database]
path = "data/zenproxy.db"                 # SQLite 数据库路径

[validation]
url = "https://www.bing.com"              # 验证目标 URL
timeout_secs = 10                         # 单个代理验证超时（秒）
concurrency = 50                          # 并发验证数
interval_mins = 30                        # 定时验证间隔（分钟）
error_threshold = 10                      # 连续失败超过此值删除代理

[quality]
interval_mins = 120                       # 质检间隔（分钟），实际不使用此字段
concurrency = 10                          # 并发质检数
```

## 验证与质检

### 代理验证（Validation）

验证通过配置的 URL 检测代理是否可用，标记为 Valid / Invalid。

**触发时机：**
- 导入/刷新订阅后**立即触发**
- 定时任务：每 `validation.interval_mins` 分钟运行一次

**流程：**
1. `sync_proxy_bindings(Validation)` — 优先为 Untested 代理分配端口
2. 并发验证所有有端口的 Untested 代理
3. 成功 → Valid，失败 → Invalid
4. 如果 Untested 数量 > `max_proxies`，多轮循环直到全部验证完
5. 无法获取绑定的代理（配置错误）直接标记为 Invalid
6. 验证完成后执行 `sync_proxy_bindings(Normal)` 恢复正常端口分配

**并发安全：** 全局 `validation_lock` 保证同一时间只有一个验证/质检任务运行，后续请求排队等待。

### 质量检测（Quality Check）

通过 ip-api.com 和 ipinfo.io 获取代理的 IP 信息、地理位置、风险评估。

**触发时机：**
- 启动后 60 秒开始第一轮
- 每轮结束后：有代理被检查 → 等 30 秒；无需检查 → 等 300 秒（5 分钟）
- 可通过管理后台手动触发

**检测内容：**
| 项目 | 来源 | 说明 |
|------|------|------|
| IP 地址 | ip-api.com / ipinfo.io | 代理出口 IP |
| 国家 | ip-api.com / ipinfo.io | 国家代码 |
| IP 类型 | ipinfo.io | ISP / Datacenter 等 |
| 是否住宅 | ipinfo.io | company.type == "isp" |
| ChatGPT 可访问 | chatgpt.com | 检测是否被封锁 |
| Google 可访问 | google.com/generate_204 | 检测连通性 |
| 风险评分 | ip-api.com | proxy + hosting 综合评分 |

**重试策略：** 缺少国家、IP 类型、IP 地址或风险等级为 Unknown 的代理，下次循环自动重试。

**过期策略：** 质检数据超过 24 小时自动重新检测。

## 认证方式

ZenProxy 支持三种认证方式：

| 方式 | 适用场景 | 格式 |
|------|----------|------|
| OAuth 会话 | Web 页面 | Cookie: `zenproxy_session=...`（7 天有效） |
| API Key | 程序调用 | Query: `?api_key=xxx` 或 Header: `Authorization: Bearer xxx` |
| 管理密码 | 管理后台 | Header: `Authorization: Bearer {admin_password}` |

OAuth 使用 [Linux.do](https://linux.do) 作为身份提供商。用户登录后获得 API Key，可在个人页面查看和重新生成。

> **注意：** `/api/relay` 端点**仅支持 `api_key` query 参数认证**。请求中的 `Authorization`、`Cookie` 等 header 会原样转发给目标服务器，不会用于 ZenProxy 认证。这样可以避免 ZenProxy 认证信息与目标 API 认证信息冲突。

## API 接口

### 页面

| 路径 | 说明 |
|------|------|
| `GET /` | 用户页面 |
| `GET /admin` | 管理后台 |

### 认证

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| `GET /api/auth/login` | 跳转 OAuth 登录 | 无 |
| `GET /api/auth/callback` | OAuth 回调 | 无 |
| `GET /api/auth/me` | 获取当前用户信息 | 会话 |
| `POST /api/auth/logout` | 登出 | 会话 |
| `POST /api/auth/regenerate-key` | 重新生成 API Key | 会话 |

### 代理获取（/api/fetch）

```
GET /api/fetch?api_key=xxx&count=5&country=US&chatgpt=true
```

**参数：**

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `api_key` | string | - | API Key（也可用 Header） |
| `count` | int | 1 | 返回代理数量 |
| `proxy_id` | string | - | 指定代理 ID |
| `chatgpt` | bool | false | 仅返回支持 ChatGPT 的代理 |
| `google` | bool | false | 仅返回支持 Google 的代理 |
| `residential` | bool | false | 仅返回住宅 IP 代理 |
| `risk_max` | float | - | 最大风险评分（0~1） |
| `country` | string | - | 国家代码过滤（如 US、JP） |
| `type` | string | - | 代理类型过滤（vmess、vless、trojan 等） |

**响应示例：**

```json
{
  "proxies": [
    {
      "id": "uuid",
      "name": "代理名称",
      "type": "vmess",
      "server": "1.2.3.4",
      "port": 443,
      "local_port": 10002,
      "status": "valid",
      "quality": {
        "ip_address": "5.6.7.8",
        "country": "US",
        "ip_type": "ISP",
        "is_residential": true,
        "chatgpt": true,
        "google": true,
        "risk_score": 0.1,
        "risk_level": "Low"
      }
    }
  ],
  "count": 1
}
```

### 请求转发（/api/relay）

通过代理池转发任意 HTTP 请求到目标 URL。

```
POST /api/relay?api_key=xxx&url=https://api.example.com/data&method=POST&country=US
```

> **认证要求：** relay 端点**仅接受 `api_key` query 参数**认证。请求中的 `Authorization`、`Cookie` 等 header 会原样转发给目标，不会被 ZenProxy 消费。

**参数：**

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `url` | string | **必填** | 目标 URL |
| `method` | string | GET | HTTP 方法（GET/POST/PUT/DELETE/PATCH/HEAD） |
| `api_key` | string | **必填** | ZenProxy API Key（仅支持 query 参数） |
| `proxy_id` | string | - | 指定代理（支持无端口代理，按需创建绑定） |
| `chatgpt` | bool | false | ChatGPT 可用过滤 |
| `google` | bool | false | Google 可用过滤 |
| `residential` | bool | false | 住宅 IP 过滤 |
| `risk_max` | float | - | 最大风险评分 |
| `country` | string | - | 国家过滤 |
| `type` | string | - | 代理类型过滤 |

**请求转发行为：**
- 请求体（body）原样转发到目标，任何 HTTP 方法只要有 body 都会转发
- 用户请求头原样转发（仅排除 `host`、`connection`、`transfer-encoding` 等 hop-by-hop 头）
- `Authorization`、`Cookie`、`Content-Type` 等 header 全部转发给目标
- 目标服务器的响应头全部回传（包括 `Content-Encoding`、`Set-Cookie` 等）
- 响应流式返回，不缓冲

**代理选择行为：**
- 未指定代理时随机选择匹配过滤条件的代理，最多重试 5 次
- 指定无端口代理时自动按需创建 sing-box 绑定

**额外响应头：**

| Header | 说明 |
|--------|------|
| `X-Proxy-Id` | 使用的代理 ID |
| `X-Proxy-Name` | 代理名称（URL 编码） |
| `X-Proxy-Server` | 代理服务器地址 |
| `X-Proxy-IP` | 代理出口 IP |
| `X-Proxy-Country` | 代理所在国家 |
| `X-Proxy-Attempt` | 重试次数（仅随机选择时） |

**使用示例：**

```bash
# 通过美国住宅代理访问 API
curl "https://your-domain.com/api/relay?api_key=xxx&url=https://httpbin.org/ip&country=US&residential=true"

# 通过指定代理发送 POST 请求，带目标 API 的认证头
curl -X POST "https://your-domain.com/api/relay?api_key=xxx&url=https://api.example.com/data&method=POST&proxy_id=uuid" \
  -H "Authorization: Bearer target_api_token" \
  -H "Content-Type: application/json" \
  -d '{"key": "value"}'

# 通过支持 ChatGPT 的代理访问，带 Cookie
curl "https://your-domain.com/api/relay?api_key=xxx&url=https://chatgpt.com&chatgpt=true" \
  -H "Cookie: session=target_cookie"

# 目标 URL 包含 query 参数时，& 需编码为 %26
curl "https://your-domain.com/api/relay?api_key=xxx&url=https://api.example.com/search?q=test%26page=2"
```

### 代理列表（/api/proxies）

```
GET /api/proxies?api_key=xxx
```

返回所有代理及统计信息，包含质检数据。

### 订阅管理

| 方法 | 路径 | 说明 | 认证 |
|------|------|------|------|
| `GET /api/subscriptions` | 列出所有订阅 | 会话/管理 |
| `POST /api/subscriptions` | 添加订阅 | 会话/管理 |
| `DELETE /api/subscriptions/:id` | 删除订阅及其代理 | 会话/管理 |
| `POST /api/subscriptions/:id/refresh` | 刷新订阅 | 会话/管理 |

**添加订阅：**

```json
{
  "name": "订阅名称",
  "type": "auto",
  "url": "https://example.com/sub.txt"
}
```

或直接提供内容：

```json
{
  "name": "手动订阅",
  "type": "v2ray",
  "content": "vmess://...\nvless://..."
}
```

### 管理接口

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET /api/admin/stats` | 系统统计 |
| `GET /api/admin/proxies` | 代理列表 |
| `DELETE /api/admin/proxies/:id` | 删除代理 |
| `POST /api/admin/proxies/cleanup` | 清理高错误代理 |
| `POST /api/admin/validate` | 手动触发验证 |
| `POST /api/admin/quality-check` | 手动触发质检 |
| `GET /api/admin/users` | 用户列表 |
| `DELETE /api/admin/users/:id` | 删除用户 |
| `POST /api/admin/users/:id/ban` | 封禁用户 |
| `POST /api/admin/users/:id/unban` | 解封用户 |

所有管理接口需要 `Authorization: Bearer {admin_password}`。

## 部署

### 编译

```bash
# Linux 交叉编译（Windows 上使用 zig 作为链接器）
cargo zigbuild --release --target x86_64-unknown-linux-gnu

# 产物路径
target/x86_64-unknown-linux-gnu/release/zenproxy
```

### 目录结构

```
/opt/zenproxy/
├── zenproxy          # 主程序
├── sing-box          # 修改版 sing-box（同目录优先加载）
├── config.toml       # 配置文件
└── data/
    ├── zenproxy.db           # SQLite 数据库
    └── singbox-config.json   # 自动生成的 sing-box 配置
```

### 启动

```bash
cd /opt/zenproxy
./zenproxy
```

ZenProxy 启动后会：
1. 读取 `config.toml` 配置
2. 初始化 SQLite 数据库
3. 从数据库加载代理池
4. 清除旧的端口映射（sing-box 重启后旧绑定失效）
5. 启动 sing-box（最小配置），等待 API 就绪
6. 为已有的 Valid 代理创建初始绑定
7. 启动后台任务（定时验证、质检、会话清理、缓存清理）
8. 开始监听 HTTP 请求

### 后台任务

| 任务 | 间隔 | 说明 |
|------|------|------|
| 代理验证 | 每 `validation.interval_mins` 分钟 | 重新验证所有代理 |
| 质量检测 | 完成后 30s~300s | 持续循环检测未质检/过期代理 |
| 会话清理 | 每 6 小时 | 删除过期的 OAuth 会话 |
| 认证缓存清理 | 每 5 分钟 | 清理过期的认证缓存条目 |

## 日志

通过环境变量 `RUST_LOG` 控制日志级别：

```bash
# 默认
RUST_LOG=zenproxy=info,tower_http=info ./zenproxy

# 调试模式
RUST_LOG=zenproxy=debug ./zenproxy
```
