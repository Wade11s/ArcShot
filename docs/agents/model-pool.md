# Model Pools

仅当当前 harness 为 Pi 时，`arcshot-delegation` 在派遣子代理前读取本文件。本文件是 Pi harness 的模型选择唯一来源；技能文件只描述路由、并发与隔离。其他 harness 使用自身的模型选择与子代理规则，本文件不适用。

## 模型池

| 模型池 | 首选 | 备选 | 兜底 |
|---|---|---|---|
| 研究 | `openai-codex/gpt-6.1-terra` (`high`) | `xai/grok-4.7` (`xhigh`) | `stepfun/step-5-preview` (`high`) |
| 探索 | `command-code/deepseek/deepseek-v4.1-flash` (`max`) | `openai-codex/gpt-6.1-terra` (`xhigh`) | `zai-coding-cn/glm-5.3` (`max`) |
| 实现 | `command-code/deepseek/deepseek-v4.1-flash` (`max`) | `openai-codex/gpt-6.1-terra` (`high`) | `openai-codex/gpt-6-luna` (`max`) |
| 审查 | `openai-codex/gpt-6.1-sol` (`high`) | `xai/grok-4.7` (`xhigh`) | `stepfun/step-5-preview` (`high`) |

模型写作 `provider/model`，括号内为 thinking level。

## 选择规则

- **单任务**：用首选。仅当首选不可用或调用失败时，依次降级到备选、兜底；不要因个人偏好改选。
- **普通并行任务**：每个代理都用该池的首选。
- **交叉验证任务**：并发运行首选与备选，两者必须是不同模型。可用模型不足时，允许同一模型开两个代理，但报告必须标注「同模型互证」，不得称为跨模型验证。
- **派遣前核对**：确认对应 provider 已登录、模型出现在 `pi --list-models` 中。
- **显式传参**：派遣时传 `--provider`、`--model`、`--thinking`；不要回传 `$PI_PROVIDER`、`$PI_MODEL`、`$PI_REASONING_LEVEL`。
- **如实报告**：实际发生降级时，写明所用模型与原因。

## 状态

本表是初始配置，除 `kimi-coding/k3.1-pro` 在早期研究任务中实际运行过外，尚未逐项验证。表中出现不代表该模型在当前账号、thinking level 或任务类型下一定可用。

调整模型或 thinking level 时只改本表。
