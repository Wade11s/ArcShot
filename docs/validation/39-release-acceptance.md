# #39 V1 验收证据汇总与放行判定

本报告按 T-05 汇总同一候选上的功能、性能与资源证据，并给出**明确阻塞**结论：
V1 的自动化与实机可驱动项已通过，但 §4.4 用户项、真实指针手势验收（缺产品侧
AX 接缝）、原生故障注入与非 ArcShot 应用粘贴 smoke 仍未验证，因此现在不能
放行。本报告不关闭也不改写父规格 #18，不执行签名、公证、安装器或自动更新。

## 候选与环境

- 实测实现/脚本 SHA：`50719acd8b50dad42152d8cb36faa35af4900951`
  （产品代码：`fix(stitch): grow the verified surface in place and time Scroll latency
  per session`）；测试脚本 `6f3c20f735eeb8eadc5ecba30fda3c5d85f27b53`
  （含 E2E 前置加固，只改 `scripts/`、`TEST.md` 与验证文档，不含产品代码）。因此性能、
  资源与功能证据都落在同一份产品代码上。
- 时间：2026-11-05T23:20:04Z（`scripts/record-validation.sh` 采集）。
- 构建配置：Debug 与 Release。Debug 跑完整 E2E 矩阵，Release 跑 T-04 与启动冒烟；
  两者均由 `scripts/check-all.sh` / `scripts/build-app.sh` 产出，本机自签名身份 `ArcShot Dev`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建单屏 1470×956 逻辑点，scale=2。
- 权限：`accessibility=granted`、`screen_recording=granted`；未改动任何 TCC 设置。
- 夹具：自有 GPUI 精确像素夹具 `arcshot-scroll-fixture`（`agent-desktop-e2e-v1`）、
  TextEdit `textedit-36-v1`、Safari `file://` `safari-36-v1`。
- 日志：`target/e2e-full-60-run7.log`、`target/e2e-full-60-run6.log`、
  `target/38-check-all.log`、`target/perf-38-idle.log`、`target/perf-38-scroll.log`、
  `target/perf-38-scroll-start.log`、`target/perf-38-24m-perf.log`、
  `target/perf-38-24m-resource.log`。日志是本地构建产物，不入库。

| 范围 | 结果 |
| --- | --- |
| T-04 性能与资源门槛（#38 记录） | Pass |
| E2E 完整矩阵（Debug，37 用例） | Pass |
| §4.4 用户项与真实指针手势验收 | Not tested |

## 已通过的项目（同一候选）

| 范围 | 证据 | 结果 |
| --- | --- | --- |
| T-04 全部 11 项性能门槛 | `docs/validation/38-resource-budget.md` | Pass（12 行实测，含 24M 与普通文档两种场景） |
| RESOURCE-MAX | 同上 | Pass（观测 peak physical footprint 1593344 KiB = 1.52 GiB ≤ 2 GiB；`Malloc Large` 970 MB） |
| PERF-EXPORT-24M | 同上 | Pass（连续 5 次 597610/597789/595516/594349/599819 µs，PNG 5,593,285 B） |
| E2E 完整矩阵（Debug，物理/完整档） | `docs/validation/60-e2e-tiers.md` | Pass（`mac_e2e=pass … cases=37`，20 单元 37 用例一次运行内通过，318.8 s） |
| 仓库完整入口 | `scripts/check-all.sh`（含 `ARCSHOT_E2E_TIER=quiet`） | Pass（exit 0；fmt/clippy -D warnings/test/doc/依赖/架构/文字后端/启动/禁网/Release 启动） |
| 运行期禁网 | `scripts/check-no-network.sh`（check-all 内） | Pass（驻留期间无网络/监听 socket） |
| 崩溃/卡死/损坏输出/过期结果/意外落盘 | 上述运行日志 | 未观察到；E2E 另断言仓库树 PNG 文件集不变、剪贴板字节一致 |

细分用例见 `TEST.md` §2/§3（Rust 自动化）、§4.3（原生 E2E）与各票的验证记录
（`docs/validation/20…38`）。

## 阻塞项（发布前必须解决或由用户确认）

1. **真实指针手势的验收证据缺失（产品接缝缺口）。** 选区拖动/移动/调整/重选、
   对象创建/控制点拖动、端点调整在 E2E 中只能作为**前置**（`--headed` 坐标拖拽
   按 `TEST.md` §4.3 不算指针手势证据），因此 `MAC-REGION`、`MAC-ANNOTATION`、
   `MAC-TEXT` 的手势部分仍是 Not tested。来源是选区与标注对象没有 AX 节点
   （`TEST.md` §4.5 已列为需要的接缝，候选方向：选区可 AX 设置、对象可 AX 选中/
   调整）。这不是本票能自行降级的部分。
2. **§4.4 用户项未验证**：真实 TCC 授权/撤权与系统设置往返（运行中撤权、
   三项 Scroll 权限缺失通知分支）、真实中文 IME 候选窗与组合过程、视觉与审美判断
   （状态项图标、标注排版、贴边布局、密度观感、三秒完成预览动画）、多显示器与
   物理输入设备差异。
3. **原生故障注入未在实机复现**：SCK 错误/超时、捕获 worker panic、内屏断开、
   目标窗口关闭/消失的原生时序。注入式用例只证明策略分支，按 T-05 不能当作
   实机证据。
4. **非 ArcShot 应用粘贴 smoke 未执行**：MAC-COLOR-OUTPUT 要求至少一次把 PNG
   粘贴进第三方应用。E2E 只证明 PNGf 写入剪贴板、osascript 读回并在内存解码，
   没有驱动第三方应用的粘贴动作。
5. **Release 配置的完整 E2E 矩阵未运行**：本次完整矩阵用 Debug bundle；Release
   侧只跑了 T-04 测量、启动冒烟与 HUD 相关冒烟。
6. **发布前系统升级后的完整矩阵重跑**：当前结果绑定 macOS 27.2 (26B123)，
   升级后必须整矩阵重跑，不能继承。

以上 1–4 属“必测项未通过或未测”，按 #39 的放行规则即阻塞；5–6 是证据范围限制。

## 未做（本票范围外）

- 正式签名、公证、安装器、自动更新。
- 关闭或改写父规格 #18；本票只交付本报告与结论。
- 未经用户授权保存任何屏幕取证图像。

## 给下一票的接缝

- 产品侧 AX 接缝（选区 + 对象）落地后，`TEST.md` §4.3 的 MAC-E2E-12/32/35 可改为
  AX 驱动，进而让 `MAC-REGION`/`MAC-ANNOTATION`/`MAC-TEXT` 的手势部分可验收。
- 故障注入接缝：原生请求状态不确定时已有 `CaptureFailure::NativeUncertain` 分类与
  `scroll_*_failure` 文本诊断；实机注入需要可控的适配层替身，届时按 T-03 补
  MAC-NATIVE-FAIL 证据。
- 第三方粘贴 smoke：可在 TextEdit/Safari 之外另择一个明确列出的目标应用，把粘贴
  动作纳入 E2E（`osascript` 或 `agent-desktop` 的按键路径），并记录字节 hash。
