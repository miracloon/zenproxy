# /merge-dev-to-main

> Audience: AI  
> Purpose: 在**确认具备合并条件**后，执行 `dev -> main` 的本地合并与远端推送。  
> Scope: 只覆盖“检查是否应合并”与“执行合并/推送”这一个高频动作；不负责发布后的总结、review 或后续修 bug。

## 使用意图

当用户表达以下意思时，可优先读取本文件并执行：

- “把 dev 合并到 main 并推送”
- “发布当前 dev”
- “把开发分支发到 main”

这不是无条件执行命令。  
**必须先检查 `main` 与 `dev` 当前是否真的适合合并。**

## 执行前必读

先补读：

1. `AGENTS.md`
2. `docs/WORKFLOW.md`
3. 当前工作区 `git status`
4. `git log --oneline --decorate --graph -n 20`

## 决策规则

只有同时满足以下条件，才应继续执行合并：

1. 当前分支存在 `dev` 与 `main`
2. 工作区中没有会干扰合并的未提交改动
3. `dev` 相比 `main` 确实有待发布提交
4. `dev` 当前状态已经通过本地验证，至少重新执行：
   - `cargo test`
   - `cargo build`
5. 没有明显的分支倒挂或误操作迹象，例如：
   - `main` 比 `dev` 更新，且这些提交并未先回合到 `dev`
   - 当前 HEAD 并不在预期的 `dev`
   - 用户实际仍处于远程手测 / bugfix 进行中，不应发布

只要有任一条件不满足，就不要继续强行 merge；应先向用户报告检查结果与阻塞原因。

## 推荐检查顺序

按以下顺序执行检查，并在每一步读取结果后再继续：

1. `git status --short --branch`
2. `git branch --list dev main`
3. `git fetch origin`
4. `git rev-list --left-right --count main...dev`
5. `git log --oneline --decorate main..dev`
6. `cargo test`
7. `cargo build`

检查解释：

- `main...dev` 的左右计数用于确认两边谁领先
- 若 `dev` 没有领先 `main`，通常不应继续 merge
- 若 `main` 领先 `dev`，通常应先停下并检查是否需要先把 `main` 回合到 `dev`

## 执行步骤

如果检查通过，再按下面顺序执行：

1. `git checkout main`
2. `git pull --ff-only origin main`
3. `git merge --no-ff dev`
4. 再次验证：
   - `cargo test`
   - `cargo build`
5. `git push origin main`

## 严格边界

- 不要跳过“是否应合并”的检查阶段
- 不要在工作区脏的情况下直接切分支并合并
- 不要使用 `git push --force`
- 不要在 `main` 上补做与当前发布无关的代码修改
- 不要自动创建 `summary.md`
- 不要把“用户说要发布”理解成“可以忽略检查”

## 输出要求

执行本命令后，对用户的反馈应包含：

1. 是否满足合并条件
2. 如果不满足，阻塞点是什么
3. 如果已执行，实际完成了哪些 git / 验证步骤
4. `main` 最终推送到的 commit SHA
