# v0.40 Admin Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将管理后台「订阅与节点管理」页重构为节点工作台，解决代理列表过宽和操作区脱节问题，同时吸收 v0.39 已引入的 `ip_family` 约束。

**Architecture:** 继续沿用单文件 `admin.html` 的现有模式，不引入前端框架，也不把代理列表重构为卡片列表。实现上先补一个轻量的结构回归测试脚本，再逐步完成工作台骨架、双状态工具条、信息列重组与行内动作瘦身；所有运行时改动仍集中在 `src/web/admin.html`。

**Tech Stack:** Rust/Axum（页面内联 HTML 输出）, vanilla HTML/CSS/JS, Python `unittest`（结构回归测试）, Node.js（内联脚本语法检查）

---

## File Structure

### Modified Files

| File | Tasks | Changes |
| --- | --- | --- |
| `src/web/admin.html` | T02, T03, T04 | 节点工作台 DOM 结构、CSS、筛选层、双状态工具条、表格列重组、行内动作瘦身、未知态显示 |

### New Files

| File | Tasks | Purpose |
| --- | --- | --- |
| `tests/ui/test_admin_workspace.py` | T01, T02, T03, T04 | 以 `unittest` 锁定管理后台工作台结构、关键文案、表头收缩结果，以及内联 `<script>` 的 Node 语法检查 |

### No New Runtime Modules Expected

v0.40 默认不新增新的运行时 JS 文件、CSS 文件或 API 路由。页面脚本继续保留在 `src/web/admin.html` 内联，以控制改动范围。

---

## Task Carrier Decision

v0.40 使用单一 `plan.md` 承载任务边界，不拆分 `taskNN.md`。

原因：

1. 本次改动集中在一个页面文件与一个轻量测试脚本，任务边界天然连续
2. 风险点主要是 UI 结构和交互语义，不是多模块并行开发
3. exec 阶段仍应严格按任务顺序推进，并在每个明确任务单元完成后先验证、再 commit

---

## Task Dependency Graph

```text
T01 (前端结构回归测试壳)
  └─→ T02 (节点工作台骨架与样式层)
        └─→ T03 (Header / 双状态工具条 / 选择摘要)
              └─→ T04 (表格信息重组 / 行内动作瘦身 / 未知态显示)
```

**Critical path:** T01 → T02 → T03 → T04

---

## Verification Strategy

1. 前端结构回归使用仓库内 `unittest` 脚本完成，不引入 `pytest`
2. 测试脚本负责两类检查：
   - 关键 DOM 片段和文案是否存在 / 是否已移除
   - 从 `admin.html` 中提取内联 `<script>`，调用 `node --check` 做语法检查
3. 每个任务完成后至少运行目标测试；任务收口时补跑：

```bash
uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
cargo test
```

4. 最终手测需覆盖：
   - 页面顺序变为“统计卡片 → 订阅源 → 节点工作台”
   - 未选中时工具条显示全局动作
   - 选中后工具条切到选中动作
   - 表格不再出现独立 `IP / IP族 / 国家 / GPT / Google / 住宅` 多列
   - `IPv4 / IPv6` 筛选仍能工作
   - 无质检数据的代理显示为明确的未知态，而不是误导性否定态

---

## Detailed Tasks

### Task 01: 建立管理后台工作台回归测试壳

**Files:**
- Create: `tests/ui/test_admin_workspace.py`

- [ ] **Step 1: 先写失败测试，锁定 v0.40 目标结构**

  在 `tests/ui/test_admin_workspace.py` 中建立最小 `unittest` 壳，至少覆盖：

  - 页面存在 `id="proxy-workspace"` 的工作台容器
  - 页面存在 `id="proxy-workspace-header"`、`id="proxy-toolbar"`、`id="proxy-toolbar-actions"`、`id="proxy-toolbar-meta"`
  - 旧的独立 `section-title">操作<` 区块不再作为独立 section 存在
  - 旧的独立 `section-title">类型分布<` 区块不再存在
  - 代理表头中存在 `节点信息`、`端口 / 错误`、`质量标签`
  - 代理表头中不再存在独立 `IP族`、`GPT`、`Google`、`住宅` 列

  可采用如下最小结构：

  ```python
  import re
  import subprocess
  import tempfile
  import unittest
  from pathlib import Path

  ADMIN_HTML = Path("src/web/admin.html")

  class AdminWorkspaceTest(unittest.TestCase):
      def load_html(self) -> str:
          return ADMIN_HTML.read_text(encoding="utf-8")

      def test_workspace_shell_exists(self):
          html = self.load_html()
          self.assertIn('id="proxy-workspace"', html)
          self.assertIn('id="proxy-toolbar"', html)
  ```

- [ ] **Step 2: 运行目标测试，确认先失败**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: 因 `proxy-workspace`、新表头或旧 section 尚未实现而失败。

- [ ] **Step 3: 给测试脚本补上内联脚本语法检查辅助函数**

  在同一测试文件中增加：

  - 从 `src/web/admin.html` 提取 `<script>...</script>` 内容
  - 写入临时 `.js` 文件
  - 调用 `node --check` 确认语法合法

  示例：

  ```python
  def test_inline_script_has_valid_js_syntax(self):
      html = self.load_html()
      match = re.search(r"<script>(.*)</script>", html, re.S)
      self.assertIsNotNone(match)
      with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False) as f:
          f.write(match.group(1))
          path = f.name
      result = subprocess.run(["node", "--check", path], capture_output=True, text=True)
      self.assertEqual(result.returncode, 0, result.stderr)
  ```

- [ ] **Step 4: 重新运行测试，确认当前失败点仍只针对新结构缺失**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: `node --check` 通过，但工作台结构相关断言继续失败。

- [ ] **Step 5: Commit**

  ```bash
  git add tests/ui/test_admin_workspace.py
  git commit -m "test(ui): add admin workspace regression harness"
  ```

---

### Task 02: 落地节点工作台骨架与页面结构重排

**Files:**
- Modify: `src/web/admin.html`
- Test: `tests/ui/test_admin_workspace.py`

- [ ] **Step 1: 先扩展失败测试，锁定 DOM 骨架与 section 顺序**

  在 `tests/ui/test_admin_workspace.py` 增加断言，要求：

  - `订阅源` section 后直接出现 `proxy-workspace`
  - `proxy-workspace` 内部包含：
    - `id="proxy-workspace-header"`
    - `id="proxy-filter-bar"`
    - `id="proxy-toolbar"`
    - 现有代理表格容器
  - 类型 chips 移入 `id="workspace-type-chips"`

  可增加基于字符串顺序的最小断言：

  ```python
  self.assertLess(html.index('section-title">订阅源<'), html.index('id="proxy-workspace"'))
  self.assertIn('id="workspace-type-chips"', html)
  ```

- [ ] **Step 2: 运行目标测试，确认先失败**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: 因工作台骨架和类型 chips 迁移尚未实现而失败。

- [ ] **Step 3: 在 `admin.html` 中完成工作台骨架重排**

  具体实现要求：

  - 删除独立的“操作”大 section
  - 删除独立的“类型分布”section
  - 在订阅源 section 之后创建新的工作台容器：

  ```html
  <div class="section" id="proxy-workspace">
    <div class="workspace-shell">
      <div class="workspace-header" id="proxy-workspace-header">
        <div class="workspace-title-block">...</div>
        <div class="type-chips" id="workspace-type-chips"></div>
      </div>
      <div class="filter-bar workspace-filters" id="proxy-filter-bar">...</div>
      <div class="workspace-toolbar" id="proxy-toolbar">
        <div id="proxy-toolbar-meta"></div>
        <div class="btn-group" id="proxy-toolbar-actions"></div>
      </div>
      <div class="card">...</div>
    </div>
  </div>
  ```

  - CSS 新增 `workspace-shell`、`workspace-header`、`workspace-filters`、`workspace-toolbar` 等类
  - `loadStats()` 中把类型 chips 的目标容器从旧 `type-chips` 改为 `workspace-type-chips`

- [ ] **Step 4: 运行目标测试确认通过**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: 工作台骨架、section 顺序、类型 chips 迁移相关测试通过。

- [ ] **Step 5: Commit**

  ```bash
  git add src/web/admin.html tests/ui/test_admin_workspace.py
  git commit -m "feat(admin-ui): add proxy workspace shell"
  ```

---

### Task 03: 实现工作台 Header 与双状态工具条

**Files:**
- Modify: `src/web/admin.html`
- Test: `tests/ui/test_admin_workspace.py`

- [ ] **Step 1: 先写失败测试，锁定 Header 摘要与工具条状态机接口**

  在 `tests/ui/test_admin_workspace.py` 增加断言，要求脚本中存在以下辅助函数或标识：

  - `function renderWorkspaceHeader`
  - `function renderWorkspaceToolbar`
  - `id="proxy-toolbar-meta"`
  - `id="proxy-toolbar-actions"`
  - 选中模式文案：`已选`
  - 全局模式文案：`当前筛选`

  示例：

  ```python
  self.assertIn("function renderWorkspaceHeader(", html)
  self.assertIn("function renderWorkspaceToolbar(", html)
  self.assertIn("当前筛选", html)
  self.assertIn("已选", html)
  ```

- [ ] **Step 2: 运行目标测试，确认先失败**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: 因新的 Header / Toolbar 渲染辅助函数尚未存在而失败。

- [ ] **Step 3: 在 `admin.html` 中补齐 Header 与工具条渲染函数**

  在页面脚本中新增并接入以下函数：

  - `renderWorkspaceHeader(filtered)`
  - `renderWorkspaceToolbar(filtered)`
  - `getSelectionSummary(filtered)` 或同等职责 helper

  行为要求：

  - `selectedIds.size === 0` 时渲染全局动作组
  - `selectedIds.size > 0` 时切换为选中动作组
  - 工具条左侧始终显示当前筛选结果数
  - 选中模式必须显示已选数量与“取消选择”
  - `renderProxies()` 与 `updateSelectionUI()` 最终都统一通过 `renderWorkspaceHeader` / `renderWorkspaceToolbar` 刷新界面

- [ ] **Step 4: 为工具条补齐桌面端轻量 sticky 样式**

  在 CSS 中增加：

  - `position: sticky`
  - 合理 `top`
  - 轻量背景与边框

  约束：

  - 仅限桌面端生效
  - 不得变成强吸顶导航条

- [ ] **Step 5: 运行目标测试确认通过**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: Header / Toolbar 状态机相关测试通过，脚本语法检查继续通过。

- [ ] **Step 6: Commit**

  ```bash
  git add src/web/admin.html tests/ui/test_admin_workspace.py
  git commit -m "feat(admin-ui): add contextual workspace toolbar"
  ```

---

### Task 04: 重组表格信息列、压缩行内动作并明确未知态

**Files:**
- Modify: `src/web/admin.html`
- Test: `tests/ui/test_admin_workspace.py`

- [ ] **Step 1: 先写失败测试，锁定新表头和行内动作策略**

  在 `tests/ui/test_admin_workspace.py` 增加断言，要求：

  - 新表头为：
    - `名称`
    - `节点信息`
    - `状态`
    - `端口 / 错误`
    - `质量标签`
    - `操作`
  - 不再存在独立 `类型`、`服务器`、`IP`、`IP族`、`国家`、`GPT`、`Google`、`住宅` 列
  - 行内动作中存在 `更多`
  - 未知态文案存在 `未质检`

  示例：

  ```python
  self.assertIn(">节点信息<", html)
  self.assertIn(">端口 / 错误<", html)
  self.assertIn(">质量标签<", html)
  self.assertNotIn(">IP族<", html)
  self.assertIn("未质检", html)
  self.assertIn("更多", html)
  ```

- [ ] **Step 2: 运行目标测试，确认先失败**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  ```

  Expected: 因表头、未知态和“更多”菜单尚未完成而失败。

- [ ] **Step 3: 在 `admin.html` 中重写代理表头与行渲染 helper**

  建议新增以下渲染 helper：

  - `renderNodeInfoCell(p, q)`
  - `renderPortErrorCell(p)`
  - `renderQualityTags(q)`
  - `renderRowActions(p, isChecking)`

  具体要求：

  - `节点信息` 合并：
    - `type`
    - `server:port`
    - `ip_address`
    - `ip_family`
    - `country`
  - `端口 / 错误` 合并：
    - 本地端口
    - 错误数
  - `质量标签` 合并：
    - `GPT`
    - `Google`
    - `住宅`
    - 风险 badge

- [ ] **Step 4: 实现未知态显示语义**

  在 `renderQualityTags(q)` 中明确区分：

  - `q == null` 时显示 `未质检`
  - `q.ip_address` 存在但 `q.ip_family` 缺失时显示 `IP族未知`
  - 风险未知时显示弱化 badge，而不是等同于失败

  约束：

  - 不得把未知态渲染成红色否定标签
  - `IPv4 / IPv6` 仅在 `q.ip_family` 有值时展示

- [ ] **Step 5: 将行内动作压缩为“主动作 + 更多”**

  具体要求：

  - 常驻主动作保留：
    - `启用 / 禁用`
    - `连通测试`
  - 次级动作移入 `更多`
    - `质量检测`
    - `删除`

  呈现形式建议优先使用原生 `details/summary`：

  ```html
  <div class="actions-cell">
    <button class="btn-xs btn-info">连通测试</button>
    <details class="row-actions-menu">
      <summary class="btn-xs">更多</summary>
      <div class="row-actions-popover">...</div>
    </details>
  </div>
  ```

- [ ] **Step 6: 运行全量前端结构测试与 Rust 测试**

  Run:

  ```bash
  uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'
  cargo test
  ```

  Expected:

  - `unittest` 全部通过
  - `cargo test` 无回归

- [ ] **Step 7: 手动检查管理后台页面**

  至少验证：

  - 工具条在未选中 / 已选中之间切换正常
  - `IPv4 / IPv6` 质量筛选仍有效
  - 窄屏下筛选层和工具条能折行
  - 行内 `更多` 不遮挡相邻行

- [ ] **Step 8: Commit**

  ```bash
  git add src/web/admin.html tests/ui/test_admin_workspace.py
  git commit -m "feat(admin-ui): regroup proxy table and slim row actions"
  ```

---

## Final Verification Checklist

- [ ] `uv run python -m unittest discover -s tests/ui -p 'test_admin_workspace.py'`
- [ ] `cargo test`
- [ ] 管理后台手动检查通过
- [ ] 工作树仅包含本次 UI 改动与测试文件

---

## Notes For Execution

1. `src/web/admin.html` 已超过 1100 行，执行时优先增加局部 helper，避免一次性重写整段脚本
2. 不要在 v0.40 顺手改用户页；用户页的 `ip_family` 展示只作为现有约束背景
3. 若执行中发现仅靠内联脚本难以维持可读性，可在不改变页面行为的前提下做小规模函数抽离，但不要把本次任务扩展成静态资源体系重构
4. 工具条切换逻辑与跨页选中语义强绑定，任何“清空选中”的行为都必须保持显式，而不能在筛选 / 翻页时静默重置
