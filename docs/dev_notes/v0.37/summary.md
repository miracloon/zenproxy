# v0.37 Summary

> 事实归档。不评价质量、不提改进建议。

## 交付概览

v0.37 完成了代理状态模型解耦、管理后台多选与批量操作、质检安全网、分页配置，以及后续为 `dev` / `main` 分支引入的 GitHub Actions 语义控制与实际 workflow 落地。

## 完成项

### 计划内任务

| Task | 内容 | 状态 | Commit |
|------|------|------|--------|
| T01 | Status Model Decouple | ✅ | `b461052` |
| T02 | Single Operation Fixes + Safety Net | ✅ | `b461052` |
| T03 | Batch API + Validate Invalid | ✅ | `b461052` |
| T04 | Frontend: Status Display + Actions | ✅ | `b461052` |
| T05 | Frontend: Multi-Select + Batch | ✅ | `b461052` |
| T06 | Frontend: Pagination + Polish | ✅ | `b461052` |

### 计划外补充

| 内容 | Commit |
|------|--------|
| 增加 `dev` / `main` 的 GitHub Actions 行为意图控制文档 | `74bd18f` |
| 增加根目录 AI 命令文档 `/merge-dev-to-main` | `74bd18f` |
| 将 `.github/workflows/docker.yml` 落地为 `dev=验证`、`main/tag=发布` 的双路径 CI | `ee05934` |
| 修复 `src/parser/clash.rs` 中依赖本地 `C:/tmp/sc0.yaml` 的测试，改为仓库内自包含样例 | `b461052` |

## 实际完成情况

### 后端

- 移除 `ProxyStatus::Disabled`，将验证状态与启用状态解耦
- `set_disabled()` 不再改写验证状态
- `load_from_db()` 不再用 `is_disabled` 覆盖 `status`
- 单代理连通测试 / 质检支持端口记忆
- 新增 `POST /api/admin/proxies/batch`
- 新增 `POST /api/admin/proxies/validate-invalid`
- 新增 `validate_invalid_only()` 批量验证入口
- 质检加入 “all probes failed” 安全网，失败时自动回写为 `Invalid`
- 批量质检范围扩展到 disabled + valid 代理（通过临时端口）

### 前端

- 代理表状态展示改为“验证状态 badge + 已禁用 badge”
- 过滤栏拆分为“验证状态”和“启用状态”两个独立下拉
- 操作列固定为四按钮：启用/禁用、连通测试、质量检测、删除
- 新增单代理验证/质检并发限制
- 新增多选状态管理、全选、选本页、取消选择
- 新增选中批量操作区
- 新增四分区操作布局
- 新增分页大小选择与 `localStorage` 持久化

### 文档与 CI 控制

- `docs/WORKFLOW.md` 明确 `dev=验证型 CI`、`main=发布型 CI`
- `docs/SPEC.md` 增加 GitHub Actions 控制边界与 AI 命令文档边界
- 根目录新增 `merge-dev-to-main.command.md`
- `.github/workflows/docker.yml` 现在同时监听 `dev`、`main`、`v*`
- `dev` 路径只做验证，不推送镜像
- `main/tag` 路径执行正式 Docker 发布

## 偏移记录

| 偏移 | 说明 |
|------|------|
| T01~T06 未按 task 边界形成多条实现提交 | 实际形成一个聚合实现提交 `b461052`，文档基线为 `2668668` |
| v0.37 闭环阶段追加了控制文档与 CI workflow 变更 | 这部分不在原 `plan.md` 的功能范围内，但属于版本收口时新增的分支/发布控制补充 |

## 未完成项

| 项 | 说明 |
|----|------|
| 远程部署与手动测试 | 本次只完成本地开发闭环；远程部署后的行为确认尚未在本 summary 中归档 |
| `review.md` | 用户未触发 review 阶段，本次未产出 |

## 提交记录

```text
ee05934 ci: split dev verification from main release
74bd18f docs: clarify dev-main CI intent and add merge command
b461052 feat(v0.37): decouple proxy status and add batch admin ops
2668668 docs(v0.37): add locked design and implementation plan
```

## 验证状态

| 验证项 | 结果 |
|--------|------|
| `cargo test` | ✅ 通过（5 passed） |
| `cargo build` | ✅ 通过 |
| `admin.html` 脚本语法检查 | ✅ 通过 |
| workflow 结构检查 | ✅ 已确认 `dev` / `main` / `tag` 触发与 `verify-*` / `release-*` job 分离 |
| 远程部署手测 | ⏳ 未纳入本次 summary |
