---
name: arcshot-git
description: ArcShot 的分支与 worktree 规则：命名、基点、worktree 位置、合并与 PR、研究与原型分支保留、提交规范与清理。创建或删除分支和 worktree、合并、开 PR、提交、清理工作区时使用。
---

# ArcShot Git Workflow

## 分支

| 用途 | 分支名 | 基点 | 去向 |
|---|---|---|---|
| 单票据实现 | `ticket/<issue>-<slug>` | `main` | PR 合并到 `main` |
| 多票据集成 | `spec/<slug>` | `main` | 最终 PR 合并到 `main` |
| 集成中的单票据 | `ticket/<issue>-<slug>` | `spec/<slug>` | 合并回集成分支 |
| 研究 | `research/<name>` | `main` | 不合并，永久保留 |
| 原型 | `prototype/<name>` | `main` | 不合并，永久保留 |

票据编号来自 GitHub issue；slug 用简短英文小写连字符。

## Worktree

- 会写文件的并发任务，每个代理一个 worktree：`.worktrees/<slug>/`，已加入 `.gitignore`。
- 创建：`git worktree add -b <branch> .worktrees/<slug> <base>`。
- 只读任务不建 worktree，直接在仓库运行。
- 任务合并或放弃后删除 worktree：`git worktree remove .worktrees/<slug>`。需要保留现场时先说明原因。

## 合并与 PR

- 单票据 `/implement`：完成并通过 `/code-review` 后开 PR 到 `main`。
- 双轴审查无未决发现后，开 PR 并立即执行 `gh pr merge --auto --merge`；必需检查变绿后由 GitHub 自动合并，合并完成后 fast-forward `main` 并清理 worktree 与分支。等待用户人工原生验收（MAC-*-MANUAL 类）或用户明确要求先审阅的 PR 不自动合并，须用户确认后再执行。
- `/implement-spec`：实现代理从 `spec/<slug>` 拉票据分支，合并回集成分支；整体验收后开一个 PR 到 `main`。
- 所有提交必须经 PR；`main` 的 ruleset 强制 PR 与必需状态检查，纯文档或配置修正同样走 PR。
- 不 force-push `main`；不改写已推送的共享分支。

## 研究与原型

- `research/*`、`prototype/*` 永不合并到 `main`，作为一手资料保留，并在对应票据中链接。
- 当结论会在实现阶段被反复查阅时，把提炼后的内容提升为 `main` 上的 `docs/research/<topic>.md`，并在票据中注明原始分支。原始报告不整体合并。

## 提交

- 使用 Conventional Commits：`feat:`、`fix:`、`docs:`、`test:`、`refactor:`、`chore:`。
- 正文引用票据编号，例如 `Refs #12`；完成时用 `Closes #12`。
- 一个提交只做一件事；测试与实现可以在同一票据的多个提交中分开。
