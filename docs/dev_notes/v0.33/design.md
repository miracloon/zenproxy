# v0.33 Design

> **状态**：design confirmed，待写 implementation plan。

---

## 版本范围

v0.33 实现两项功能：

1. **统一配置管理** — DB + config.toml + admin UI 三层同步配置体系
2. **用户管理增强** — 注册开关、OAuth 开关、登录页面重构

---

## §1 统一配置管理

### 设计理念

将配置项按"能否运行时变更"分为两类，分别由不同机制管理，三者之间**零覆盖、零交叉**：

| 类别 | 管理机制 | 特征 |
|---|---|---|
| **启动配置** | config.toml（只读） | 物理上无法运行时变更（端口、路径等） |
| **运行配置** | DB settings 表 + config.toml 双向同步 | 业务参数，可通过 admin UI 修改 |
| **容器配置** | `.env`（Docker 层面） | 端口映射、日志级别 |

### 数据同步模型

```
启动时:  config.toml ──读取──→ 写入 DB settings（config.toml 作为种子）
                                ↓
运行时:                        App 从 DB 读配置（不缓存）
                                ↑
UI 修改:  Admin UI ──保存──→ 写 DB ──→ 回写 config.toml ──→ 刷新内存
                                
手动改 config:  编辑 config.toml → 重启容器 → config.toml 重新种子到 DB → 生效
```

**核心规则**：
- 启动时 config.toml **始终覆盖** DB（config.toml 是启动时的仲裁者）
- UI 修改时同时写 DB **和** 回写 config.toml（两边始终一致）
- 运行中 app 从 DB 读取（不使用内存缓存，SQLite 读一行是微秒级）
- UI 有"保存配置"按钮，**不使用热加载**——未保存的修改仅为 UI 本地状态

### 配置项分类

#### 启动配置（config.toml only，不进 DB）

```toml
[server]
host = "0.0.0.0"
port = 3000

[singbox]
binary_path = "/app/sing-box"
config_path = "data/singbox-config.json"
base_port = 10001
max_proxies = 300
api_port = 9090

[database]
path = "data/zenproxy.db"
```

这些字段物理上决定了进程如何启动（监听地址、文件路径、端口分配），改了必须重启。

#### 运行配置（config.toml + DB + UI）

```toml
[server]
admin_password = "change-me"
min_trust_level = 1
allow_registration = false
enable_oauth = true

[oauth]
client_id = ""
client_secret = ""
redirect_uri = "https://your-domain.com/api/auth/callback"

[singbox]
api_secret = ""

[validation]
url = "https://www.bing.com"
timeout_secs = 10
concurrency = 50
interval_mins = 30
error_threshold = 10
batch_size = 30

[quality]
interval_mins = 120
concurrency = 10

[subscription]
auto_refresh_interval_mins = 0
```

这些字段通过 admin UI 的 Settings 面板可修改，保存时同步写 DB 和 config.toml。

#### 容器配置（.env only，不进 config.toml 和 DB）

```env
SERVER_PORT=3000
SINGBOX_API_PORT=9090
PROXY_PORT_START=10002
PROXY_PORT_END=10301
RUST_LOG=zenproxy=info,tower_http=info
```

仅用于 docker-compose 的宿主机端口映射和日志级别，不进入应用层。

### DB 层改动

**文件**：`src/db.rs`

1. 新增 `settings` 表：

```sql
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
)
```

2. 新增方法：
   - `get_setting(key: &str) -> Option<String>`
   - `set_setting(key: &str, value: &str)`
   - `get_all_settings() -> HashMap<String, String>`
   - `set_all_settings(settings: &HashMap<String, String>)`

### Config 层改动

**文件**：`src/config.rs`

1. `AppConfig` 结构体拆分成两部分：
   - `BootConfig`：启动时读取，不可变（server.host/port, singbox, database）
   - `RuntimeConfig`：运行时可变的字段集合

2. 新增 `seed_settings_to_db(db: &Database, config: &AppConfig)` 函数：
   - 启动时从 config.toml 读取所有运行配置字段 → 写入 DB settings 表
   - 使用 `INSERT OR REPLACE` 确保 config.toml 始终覆盖

3. 新增 `write_settings_to_config(settings: &HashMap<String, String>, path: &str)` 函数：
   - 使用 `toml_edit` crate 修改 config.toml
   - 保留文件中的注释和格式

4. Cargo.toml 新增依赖：`toml_edit`

### AppState 改动

**文件**：`src/main.rs`

```rust
pub struct AppState {
    pub boot_config: BootConfig,       // 启动配置（不可变）
    pub config_path: PathBuf,          // config.toml 路径（用于回写）
    pub db: Database,
    // ... 其他字段不变
}
```

运行时需要读配置的地方，从 `state.boot_config.xxx` 读启动配置，从 `state.db.get_setting("xxx")` 读运行配置。

### 后台任务改动

所有后台任务（验证、质检、订阅刷新）在**每轮循环开始时**从 DB 重新读取配置参数：

```rust
// 现有代码（使用启动时固定值）
let interval = Duration::from_secs(state.config.validation.interval_mins * 60);

// 改为（每轮重读 DB）
let interval_mins: u64 = state.db.get_setting("validation_interval_mins")
    .and_then(|v| v.parse().ok())
    .unwrap_or(30);
let interval = Duration::from_secs(interval_mins * 60);
```

**已知限制**：如果任务正在 sleep 30 分钟，UI 改成 5 分钟，需要等当前 sleep 结束后下一轮才生效。可接受。

### API 层改动

**文件**：`src/api/admin.rs`

新增两个 endpoint（admin 路由下，需 admin_password 认证）：

1. `GET /api/admin/settings` — 获取所有运行配置
   - 从 DB settings 表读取
   - 返回 JSON: `{ "admin_password": "...", "min_trust_level": 1, ... }`

2. `PUT /api/admin/settings` — 批量更新运行配置
   - 请求体：`{ "min_trust_level": "2", "allow_registration": "true", ... }`
   - 操作步骤：
     1. 批量写入 DB settings 表
     2. 回写 config.toml（`toml_edit` 保留格式）
     3. 返回 200 OK
   - 全部成功才返回 200，任一步失败返回 500

### admin_password 的特殊处理

1. **首次启动**：DB 没有 `admin_password` → 从 config.toml 种子
2. **UI 修改**：admin 在 Settings 面板修改密码 → 写 DB + 回写 config.toml
3. **验证逻辑**：`admin_auth` middleware 从 DB 读取 `admin_password` 验证（不再从 `state.config` 读）
4. **注意**：修改 admin_password 后当前 session 不受影响（admin 后台使用 localStorage 存储密码，刷新页面后需用新密码登录）

### 前端层改动

**文件**：`src/web/admin.html`

在"操作"区域和"用户管理"区域之间，新增 **Settings 面板**：

```
┌──────────────────────────────────────────────┐
│  系统配置                        [ 保存配置 ] │
├──────────────────────────────────────────────┤
│                                              │
│  ┌─ 认证设置 ─────────────────────────────┐  │
│  │ 管理员密码:     [••••••••           ]  │  │
│  │ 最低信任等级:   [1                  ]  │  │
│  │ 允许用户注册:   [☐]                    │  │
│  │ 启用 OAuth:    [☑]                    │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌─ OAuth 配置 ───────────────────────────┐  │
│  │ Client ID:     [                    ]  │  │
│  │ Client Secret: [                    ]  │  │
│  │ Redirect URI:  [                    ]  │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌─ 验证配置 ─────────────────────────────┐  │
│  │ 验证 URL:      [https://www.bing.com]  │  │
│  │ 超时(秒):      [10                 ]  │  │
│  │ 并发数:        [50                 ]  │  │
│  │ 间隔(分钟):    [30                 ]  │  │
│  │ 错误阈值:      [10                 ]  │  │
│  │ 批量大小:      [30                 ]  │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌─ 质检配置 ─────────────────────────────┐  │
│  │ 间隔(分钟):    [120                ]  │  │
│  │ 并发数:        [10                 ]  │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌─ 订阅配置 ─────────────────────────────┐  │
│  │ 自动刷新间隔(分): [0  ] (0=关闭)       │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  状态: ● 有未保存的修改                       │
│                              [ 保存配置 ]    │
└──────────────────────────────────────────────┘
```

**交互逻辑**：
- 页面加载时调用 `GET /api/admin/settings` 填充表单
- 用户修改任何字段 → 显示"有未保存的修改"提示
- 点击"保存配置" → 收集所有字段值 → `PUT /api/admin/settings` → 成功显示"✅ 配置已保存"
- 未保存时离开页面 → 浏览器标准 `beforeunload` 提示

### docker-compose 改动

**文件**：`docker/server/docker-compose-remote.yml`

```yaml
volumes:
  - ./config/config.toml:/app/config.toml  # 移除 :ro，允许 app 回写
  - ./data:/app/data
```

原来 config.toml 挂载为 `:ro`（只读），需要改为可读写，否则 UI 保存时无法回写。

---

## §2 用户管理增强

### 新增配置项

在 `config.toml` 的 `[server]` section 新增：

```toml
[server]
# ... 现有字段 ...
allow_registration = false   # 是否允许密码用户自助注册
enable_oauth = true          # 是否启用 OAuth 登录
```

对应 DB settings 的 key：`allow_registration`、`enable_oauth`

### 登录页面重构

**文件**：`src/web/user.html`

**当前布局**：
```
┌─────────────────────┐
│     ZenProxy        │
│   选择登录方式       │
│                     │
│ [使用 Linux DO 登录] │  ← OAuth 在上方
│      —— 或 ——       │
│ [用户名         ]   │
│ [密码           ]   │
│ [  密码登录  ]      │
└─────────────────────┘
```

**重构后布局**（根据配置动态渲染）：

```
┌─────────────────────────┐
│       ZenProxy          │
│     选择登录方式         │
│                         │
│  [用户名            ]   │  ← 密码登录始终在上方
│  [密码              ]   │
│  [   密码登录    ]      │
│  [注册新账号]           │  ← 仅 allow_registration=true 时显示
│                         │
│       —— 或 ——          │  ← 仅 enable_oauth=true 时显示
│  [使用 Linux DO 登录]   │  ← 仅 enable_oauth=true 时显示
└─────────────────────────┘
```

**动态逻辑**：
- 登录页加载时先调用一个新的公开 API 获取登录选项
- 根据返回值决定显示哪些元素

### 注册页面

当 `allow_registration = true` 时，登录页显示"注册新账号"链接，点击后显示注册表单：

```
┌─────────────────────────┐
│       ZenProxy          │
│     注册新账号           │
│                         │
│  [用户名            ]   │
│  [密码              ]   │
│  [确认密码          ]   │
│  [   注册    ]          │
│                         │
│  已有账号？ 返回登录     │
└─────────────────────────┘
```

可以作为登录页的一个"视图切换"实现（同一个页面内切换显示），不需要新的 HTML 文件。

### API 层改动

**文件**：`src/api/auth.rs`

1. `GET /api/auth/options` — **公开端点**（无需认证），返回登录选项
   - 从 DB 读取 `enable_oauth` 和 `allow_registration`
   - 返回：`{ "enable_oauth": true, "allow_registration": false }`
   - 登录页 JS 根据此响应动态渲染界面

2. `POST /api/auth/register` — 用户自助注册
   - 请求体：`{ "username": "...", "password": "..." }`
   - 前置检查：
     - `allow_registration` 为 false → 返回 403 Forbidden
     - 用户名已存在 → 返回 409 Conflict
     - 密码太短（<6 字符）→ 返回 400 Bad Request
   - 创建用户：UUID id, api_key, `auth_source = "password"`, `trust_level = min_trust_level`
   - 自动创建 session → 设置 cookie → 返回 201 Created
   - 注册成功后自动登录，前端 reload 进入仪表盘

**文件**：`src/api/mod.rs`

3. OAuth 路由条件化：
   - `GET /api/auth/login`（OAuth 发起）和 `GET /api/auth/callback`（OAuth 回调）始终注册路由
   - 但在 handler 内部检查 `enable_oauth`：
     - 如果 `enable_oauth = false`，`login` handler 返回 403 "OAuth login is disabled"
     - `callback` handler 同样返回 403
   - **不删除路由**（避免动态路由注册的复杂性），只在 handler 层拒绝

### admin.html 用户管理区域增强

在现有"创建密码用户"表单旁，显示当前的注册/OAuth 开关状态（只读展示，修改通过 Settings 面板）：

```
┌─ 用户管理 ──────────────────────────────────────────────┐
│                                                         │
│  当前设置: 用户注册 ❌关闭  |  OAuth 登录 ✅开启        │
│  (在"系统配置"面板中修改)                                │
│                                                         │
│  用户名: [        ]  密码: [        ]  [创建密码用户]   │
│                                                         │
│  ID | 用户名 | 昵称 | 来源 | 信任等级 | 状态 | ...      │
│  ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ──    │
└─────────────────────────────────────────────────────────┘
```

---

## 依赖关系

```
§1（统一配置管理）— 基础设施，必须先完成
    ↓
§2（用户管理增强）— 依赖 §1 的 settings 机制（读取 enable_oauth / allow_registration）
```

§2 依赖 §1 提供的 `db.get_setting()` 接口来读取功能开关。

---

## 涉及文件清单

| 文件 | 改动类型 | 说明 |
|---|---|---|
| `Cargo.toml` | 修改 | 新增 `toml_edit` 依赖 |
| `src/config.rs` | 修改 | 拆分 BootConfig/RuntimeConfig，新增种子和回写函数 |
| `src/db.rs` | 修改 | 新增 settings 表和 CRUD 方法 |
| `src/main.rs` | 修改 | AppState 结构变更，启动流程加入 settings 种子 |
| `src/api/mod.rs` | 修改 | 新增 settings 和注册路由，admin_auth 改读 DB |
| `src/api/admin.rs` | 修改 | 新增 settings GET/PUT endpoint |
| `src/api/auth.rs` | 修改 | 新增 `/auth/options`、`/auth/register`，OAuth handler 加开关检查 |
| `src/web/user.html` | 修改 | 登录页重构：密码登录在上，动态 OAuth/注册显示 |
| `src/web/admin.html` | 修改 | 新增 Settings 面板（保存按钮），用户管理区域显示开关状态 |
| `docker/server/config/config.toml` | 修改 | 新增 `allow_registration`, `enable_oauth` 字段 |
| `docker/server/docker-compose-remote.yml` | 修改 | config.toml 挂载移除 `:ro` |
| `docker/server/docker-compose.yml` | 修改 | 同上 |

---

## 不做清单

- **不做热加载**：不监听文件变更，不用 `notify` crate，配置变更统一通过 UI "保存" 或重启容器
- **不做配置审计日志**：不记录谁在什么时候改了什么配置（未来可做）
- **不做配置导入/导出**：config.toml 本身就是导出物
- **不做 ENV 覆盖**：ENV 只管容器层面（端口、日志），不和 config.toml/DB 产生任何覆盖关系

---

## Tag 格式

从本版本开始，tag 格式改为 `v0.33`（不再使用 `v0.3.3`）。后续版本号示例：`v0.34`、`v0.35`、`v1.01`。

CI 工作流中的 semver tag 解析逻辑可能需要适配（如果有的话），需在 implementation plan 中确认。
