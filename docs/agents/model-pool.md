# Model Pools

仅当当前 harness 为 Pi 时，`arcshot-delegation` 在派遣子代理前读取本文件。本文件是 Pi harness 的模型选择来源；技能文件只描述路由、并发与隔离。

## 模型池（等权）

池内成员地位相等，没有首选、备选、兜底。模型写作 `magpie/<provider>/<model>`，括号内为该池的 thinking level。成员按额度来源分组，见下节；池间可以重叠，跨任务的均衡由选择规则按当前水位实现，不靠固定归属。

| 模型池 | 等权成员 | Thinking |
|---|---|---|
| 研究 | `magpie/kimi/k3.1-pro`、`magpie/grok/grok-4.7`、`magpie/antigravity/gemini-3.8-flash`、`magpie/stepfun-cn/step-5-preview`、`magpie/longcat/LongCat-2.5`、`magpie/deepai/mimo-2.6-pro` | `max` |
| 探索 | `magpie/deepai/glm-5.3-flash`、`magpie/deepai/deepseek-v4.1-flash`、`magpie/mistral/deepseek-v4-1-flash-1017`、`magpie/workbuddy/hy4free`、`magpie/workbuddy/deepseek-v4.1-flash`、`magpie/antigravity/gemini-3.8-flash` | `high` |
| 实现 | `magpie/kimi/k3.1-pro`、`magpie/longcat/LongCat-2.5`、`magpie/deepai/deepseek-v4.1-flash`、`magpie/deepai/mimo-2.6-pro`、`magpie/workbuddy/deepseek-v4.1-flash`、`magpie/stepfun-cn/step-5-preview` | `max` |
| 审查 | `magpie/grok/grok-4.7`、`magpie/kimi/k3.1-pro`、`magpie/stepfun-cn/step-5-preview`、`magpie/deepai/mimo-2.6-pro`、`magpie/antigravity/gemini-3.8-flash`、`magpie/longcat/LongCat-2.5` | `high` |

Thinking 档位说明：个别模型不支持池内档位时，用该模型支持的最高档，不视为降级。已知上限：`antigravity/gemini-3.8-flash` 只有 `high`；`stepfun-cn/step-5-preview`、`longcat/LongCat-2.5` 最高 `high`；`grok/grok-4.7` 最高 `xhigh`；其余成员支持 `max`。

`magpie/grok/grok-4.7-build-fast@500k` 不单独入池，只作 grok 同源内的延迟敏感替代（500K 上下文、价格较高），仅在需要该上下文或延迟时启用。

## 额度来源与成员分组

均衡的对象是额度来源，不是模型名。12 个成员分属 8 个来源：

- **kimi**：`kimi/k3.1-pro`
- **grok**：`grok/grok-4.7`、（同源替代 `grok/grok-4.7-build-fast@500k`）
- **antigravity**：`antigravity/gemini-3.8-flash`
- **stepfun-cn**：`stepfun-cn/step-5-preview`
- **longcat**：`longcat/LongCat-2.5`
- **mistral**：`mistral/deepseek-v4-1-flash-1017`
- **workbuddy**：`workbuddy/hy4free`、`workbuddy/deepseek-v4.1-flash`
- **deepai**：`deepai/glm-5.3-flash`、`deepai/deepseek-v4.1-flash`、`deepai/mimo-2.6-pro`

## DeepAI 特殊规则

DeepAI 的 Theos 额度只对付费模型生效。池内 3 个 deepai 模型均为用户选定的 free 模型：

- Theos 水位（`magpie quota` 中 deepai 的 used）**不作为这些模型的排除依据**；均衡计算时 deepai 来源水位按 0 计。
- deepai 仍受并发同源限制（见下），理由是上游可用性与限流，不是额度耗尽。
- 不要因为 `magpie quota` 显示 deepai 100% 而错杀池内 deepai 模型。

## 选择规则

### 单次派遣

1. **过滤**：池成员 ∩ `settings.json` 的 `enabledModels` ∩ `pi --list-models` 可用 ∩ 上下文窗口足够。
2. **查水位**：`magpie quota --json`（或 `GET http://127.0.0.1:3425/v1/magpie/quotas`），取每个额度来源各窗口 `used` 的最大值；余额型来源以余额判断。deepai 按上节按 0 计。
3. **挑最空来源**：水位 `<70%` 优先，`70–90%` 降权，`≥90%` 本轮排除（deepai 除外）；并列时选 `lastServedAt` 最久未用的来源。
4. **来源内选模型**：探索选 Flash 类，实现、审查选 Pro 类；仍并列时选今日用量最少的模型。多轮单任务靠此自然轮转，不固定打同一成员。

### 多并发（同一主代理同时运行 2 个子代理）

5. **不同成员、不同来源**：两个代理不能只换模型名而同属一个额度来源；必须落在两个不同额度来源。仅当过滤后可用来源不足 2 个时，才允许同来源、不同模型，并在回报里注明。
6. **交叉验证**：两个代理还必须是不同模型族（DeepSeek / GLM / Grok / Kimi / Gemini 等），且互不共享中间结论。可用来源不足时允许同来源不同模型，但报告必须标注「同来源互证」，不得称为跨模型验证。
7. **实现 + 审查配对**：审查代理避开实现代理所用的额度来源，保证独立性，即使该来源水位最低。
8. **排队任务**：前一个代理释放后重新查水位再选，不复用前一个的成员。

### 池耗尽

9. 全部来源 `≥90%`（deepai 除外）时不静默降级：依次考虑启用候选模型 → `magpie quota wait` 等重置 → 报告用户当前额度状态。

### 派遣与回报

- **派遣前核对**：确认对应 provider 已登录、模型出现在 `pi --list-models` 中。
- **显式传参**：派遣时传 `--provider`、`--model`、`--thinking`；不要回传 `$PI_PROVIDER`、`$PI_MODEL`、`$PI_REASONING_LEVEL`。
- **如实报告**：记录实际模型、额度来源、派遣时水位、thinking level，以及是否因来源不足而同来源并发。

## 不入池

- **zcode、commandcode**：额度太少，暂时不入池（`zcode/GLM-5.3`、`zcode/GLM-5.3-Flash`、`commandcode/deepseek/deepseek-v4.1-flash`）。额度恢复后按选择规则重新评估。
- **openrouter**：任何 openrouter 模型都不入池。
- **workbuddy/glm-5.3-flash**：已由 `deepai/glm-5.3-flash` 替代。
- Gateway 仅有、Pi 未配置的模型（如 `codex/gpt-6-luna`、`antigravity/gemini-4-argon`）不入池；需先在 `models.json` 的 `magpie.models` 中补加并启用。
