---
name: arcshot-delegation
description: 仅在当前 harness 为 Pi 时使用的 ArcShot 子代理派遣规则：按触发的 skill 或 workflow 选择 model pool，遵守并发上限 2，安排交叉验证与 worktree 隔离，并统一回报。Pi 中派遣子代理、选择模型、并行委派实现或审查、或要求交叉验证时使用。
---

# ArcShot Delegation

本 skill 及其引用的 `docs/agents/model-pool.md` 仅适用于 Pi harness。其他 harness 使用自身的原生子代理工具与规则，不应用这两份文档。

派遣子代理前，先读 [model pool](../../../docs/agents/model-pool.md)。会写文件的子代理，同时读 `arcshot-git` skill 创建自己的 worktree 与分支。

## 路由

| 触发入口 | 子代理任务 | 模型池 |
|---|---|---|
| `/wayfinder` 的 research 票据、`/research` | 查官方资料与源码，产出带来源的报告 | 研究 |
| `/improve-codebase-architecture`、一般代码库探索 | 搜索代码、梳理结构与摩擦，不改代码 | 探索 |
| `/code-review`、`/implement` 的收尾审查、`/rust-review` | Standards 或 Spec 独立审查，不改代码 | 审查 |
| workflow 明确要求委派的实现 | 在限定范围内实现并测试 | 实现 |
| 任一 workflow 要求交叉验证 | 同一问题由两个独立代理分析 | 原任务池内两不同额度来源的两成员 |

## 并发

同一主代理同时最多运行 **2** 个子代理。第三个任务排队；不要靠启动更多代理来加速。

- 普通并行任务：两个代理按 `model-pool.md` 的均衡规则选择不同成员、不同额度来源，各自负责不同的票据或问题。
- 交叉验证：池内两不同额度来源、两不同模型族并发；实现 + 审查配对时审查代理避开实现代理的额度来源。
- 排队任务：前一个代理释放后重新查水位再选，不复用前一个的成员。
- 会写文件的代理各自使用 worktree 与分支；同一文件或同一模块的改动串行。

## 隔离

| 任务 | 工具 | 写入 |
|---|---|---|
| 研究 | `read,bash,edit,write,grep,find` | 只写自己的报告，提交到 `research/<name>` 分支 |
| 探索 | `read,bash,grep,find,ls` | 不写文件；bash 只做查询 |
| 审查 | `read,bash,grep,find` | 不改文件；可运行测试与读取 diff |
| 实现 | 默认工具 | 限定文件范围，使用 `ticket/<issue>-<slug>` 分支 |

## 交叉验证

只有任务明确要求交叉验证时，才并发两个代理。两侧独立分析、互不共享中间结论；主代理对照后报告一致点、分歧点与证据。

## 回报

每次派遣结束记录：handle、实际模型、额度来源、派遣时水位与 thinking level、是否因来源不足而同来源并发、分支与 worktree、改动范围、执行过的验证命令、未验证项与残留风险。

子代理完成后调用 `subagent stop <handle>`；需要后续工作时才保持空闲存活。
