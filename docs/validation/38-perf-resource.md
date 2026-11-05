# #38 性能门槛与最大长图资源压力达标记录

本记录覆盖 #38 的 T-04 性能门槛与 I-13 资源上限：以完整 Release 候选实测
PERF-INTERACT / PERF-REGION / PERF-SCK / PERF-FRAME-PREVIEW /
PERF-EVENT-PREVIEW / PERF-STITCH / PERF-EXPORT-2M / PERF-EXPORT-6M /
PERF-CLIPBOARD，并测量 PERF-SCROLL-START。PERF-EXPORT-24M 与 RESOURCE-MAX
在本轮**未通过**，原因与窄跟进见下文“阻塞项”。

## 实测来源与环境

- 基线：`spec/v1` = `54f27cb`（Merge ticket/36-third-party-scroll）。本票从该
  tip 分出，实测前未合并新的 `spec/v1` tip。
- 实测实现/脚本 SHA：`9fba7f735a523abb814b71276c2001b7052fbc87`
  （`perf: add T-04 measurement seams and adaptive capture settle`）。
  测量期间工作树另有仅在拼接拒绝分支执行的临时 `stitch_debug` 诊断，提交前已
  移除；该诊断不在任何成功路径上执行，被记录的成功指标与本 SHA 一致。
- 时间：2026-11-05T05:17:47Z（最后一次测量完成之后）。
- 构建配置：Release。bundle 为 `target/bundle/release/ArcShot.app`
  （`dev.arcshot.app` / `ArcShot`，`scripts/build-app.sh --release`，
  签名身份 `ArcShot Dev`）。
- build_configuration=release
- worktree_dirty=false
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1
  (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏，1470×956 逻辑点，scale=2；`agent-desktop list-displays`
  报 bounds `1470x956`、scale `2.0`。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 为
  `accessibility=granted`、`screen_recording=granted`、`platform=macos`；
  产品预检日志 `scroll_permission screen_recording=true input_monitoring=true
  accessibility=true missing=none`。未改动 TCC。
- 夹具：`arcshot-scroll-fixture`（`crates/app/src/bin/scroll-fixture.rs`，
  760×720 逻辑点、内容 0..7400 逻辑行、初始偏移 2000），Release 构建。
- 日志：`target/perf-measure-release.log`（测量驱动写入）、
  `target/perf-scroll-fixture.log`、`target/perf-24m-fixture.log`。日志是本地
  构建产物，不入库；本报告只保存文本/数值/hash 摘要。
- 系统负载：测量期间机器有用户应用（WindowServer / WebKit / Dia / Spotlight
  `mdworker_shared` / magpie）在跑，1 分钟 load average 在 3..41 间波动。
  PERF-REGION 对负载敏感，见下文两组样本。

## 测量入口（可复现）

```sh
# 构建并驱动正式 Release bundle，采集全部文本诊断
scripts/perf-measure.py --configuration release --samples 30

# 单独复算统计与门槛（nearest-rank p95）
scripts/perf-gate.sh target/perf-measure-release.log

# 最大导出期间的资源采样（RSS 100 ms，footprint 交叉检查）
scripts/resource-footprint.sh <arcshot-pid> 0.1 <seconds>
```

所有原生门禁运行都在 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行执行，
并在每次运行前确认 `agent-desktop list-apps --app "ArcShot"` 为空。

`scripts/perf-gate.sh` 的统计方法：样本升序排序，median 取第
`int((n+1)/2)` 个，nearest-rank p95 取第 `ceil(0.95*n)` 个（1-indexed），
max 取最大值。导出样本按 `output_encode_us=… pixels=WxH` 的物理像素数分桶
（≤2M / ≤6M / ≤24M）。

## 修复的实际瓶颈：固定 80 ms 准备窗口 settle

`capture_wake_window_submit_us` 最初的中位数为 230 ms，p95 423 ms，超过
PERF-REGION 的 300 ms 门槛。分段诊断（`capture_submit_us` /
`capture_preparing_opened_us` / `capture_worker_us` / `capture_prepare_us` /
`capture_open_window_us`）显示：

- `capture_preparing_opened_us` ≈ 100 ms：`Capture` effect 打开准备窗口后，
  代码固定等待 `PREPARE_SETTLE = 80 ms` 才提交 SCK 请求；
- `capture_worker_us` ≈ 80 ms：其中 `SCShareableContent::get()`（预 SCK）在
  负载下 45..290 ms，SCK 调用本身 30..40 ms；
- `capture_prepare_us` ≈ 1 ms，`capture_open_window_us` ≈ 12 ms。

修复把固定 sleep 替换为**轮询准备窗口在 WindowServer 的注册**：`open_preparing`
读取准备窗口的 NSWindow number，`tick` 每轮检查该 number 是否出现在
`owned_visible_window_ids()` 中，出现即提交；`PREPARE_SETTLE` 仍作为绝对上限。
注册检测在稳态下约 15..35 ms，省去约 80 ms。修复后 PERF-REGION 中位数
230 ms → 144 ms，p95 423 ms → 166 ms（低负载组）。

`SCShareableContent::get()` 的预 SCK 成本随系统负载波动（45..290 ms），是
PERF-REGION 残余 p95 的主要来源；本轮未缓存该对象（缓存会引入过期窗口映射
导致的 `CaptureError::Window`/`Display` 风险），列为窄跟进。

## 原始样本与统计

> 驱动每次运行会清空并重写 `target/perf-measure-release.log`。下表为各次运行
> 的 `scripts/perf-gate.sh` 输出统计（原样本在该次运行的日志中；命令可按上
> 节复现）。PERF-REGION 给两组：低负载的 `region_sck` 组与负载更高的
> `clipboard_export` 组。

| 指标 | 运行 | n | median_us | p95_us | max_us | 门槛 | 结果 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PERF-INTERACT | interact | 31 | 0 | 1 | 1 | p95 ≤ 100000 | Pass |
| PERF-INTERACT | clipboard_export | 120 | 1 | 6 | 274 | p95 ≤ 100000 | Pass |
| PERF-REGION | region_sck | 31 | 144202 | 165824 | 305890 | p95 ≤ 300000 | Pass |
| PERF-REGION | clipboard_export | 61 | 167944 | 298076 | 550325 | p95 ≤ 300000 | Pass |
| PERF-SCK | region_sck | 31 | 31984 | 38313 | 125437 | p95 ≤ 150000 | Pass |
| PERF-SCK | clipboard_export | 61 | 40133 | 59845 | 272397 | p95 ≤ 150000 | Pass |
| PERF-FRAME-PREVIEW | scroll | 80 | 115108 | 131088 | 134880 | p95 ≤ 200000，max ≤ 500000 | Pass |
| PERF-EVENT-PREVIEW | scroll | 79 | 149058 | 283919 | 300866 | p95 ≤ 400000，max ≤ 750000 | Pass |
| PERF-STITCH | scroll | 82 | 800 | 819 | 898 | p95 ≤ 100000 | Pass |
| PERF-EXPORT-2M | clipboard_export | 30 | 34445 | 43548 | 61678 | p95 ≤ 2000000 | Pass |
| PERF-EXPORT-6M | clipboard_export | 30 | 47034 | 59241 | 117754 | p95 ≤ 5000000 | Pass |
| PERF-CLIPBOARD | clipboard_export | 60 | 461 | 609 | 1104 | p95 ≤ 500000 | Pass |
| PERF-SCROLL-START | scroll | 1 | 422957 | 422957 | 422957 | p95 ≤ 1000000 | Not tested（n<30） |
| PERF-EXPORT-24M | export_24m | 0 | - | - | - | 连续 5 次每次 ≤ 20000000 | Not tested（阻塞） |
| RESOURCE-MAX | export_24m | 0 | - | - | - | 观测 peak ≤ 2 GiB | Not tested（阻塞） |

PERF-INTERACT 的 `interact_commit_us` 口径为命令出队到 session 提交完成；
UI 命令队列由 16 ms 的 UI tick 排空，因此真实输入到提交还包含 ≤16 ms 的排队
分量。即使加上该上界，p95 仍远低于 100 ms。

## 阻塞项：最大 24M 文档无法经内嵌预览拼接

PERF-EXPORT-24M 与 RESOURCE-MAX 需要一份 24,000,000 物理像素的文档。自有夹具
内容高 8120 逻辑行（16240 物理行），选区宽 1500 物理像素时文档上限为
`floor(24_000_000 / 1500) = 16000` 物理行。要达到该高度，选区必须几乎覆盖夹具
全宽（≥739 逻辑点），因此固定预览只能放在选区内侧（`placed=inner`）。

实测中（Release、`--dy 1`/`--dy 5` 慢速滚动），初始帧与滚动后的帧在
`capture_probe_grid` 的 25 个采样点上与夹具确定性内容**逐点一致**，但
`StitchBuilder::submit` 返回 `ContentChanged`：最佳候选位移正确（如 -32 物理
行），重叠 328 行中 280 行匹配、48 行不匹配，首处不匹配在文档第 0 行附近。
即捕获帧在稀疏探针上正确，但逐行比较在顶部约 48 行不一致。内嵌预览窗口被
`scroll_exclusion_fixed` 排除，但其覆盖区域（选区右部全高）的逐行内容仍有
部分不匹配，疑似窗口阴影/边框或排除滤镜在窗口边界处的合成差异。

**窄跟进（阻塞 #38 验收）**：修复内嵌固定预览下滚动帧的逐行拼接验证。可行
方向：(1) 在捕获适配层记录排除窗口边界处的原始行 hash，定位是阴影、边框还是
排除滤镜 hole；(2) 若为 GPUI pop-up 阴影，考虑 `igNoShadow` 或对排除窗口的
shadowPath 做处理；(3) 增加一个不依赖内嵌预览的 24M 文档构建路径（例如更宽的
第三方目标或可配置夹具几何），并在同一最终候选重跑 PERF-EXPORT-24M 与
RESOURCE-MAX。在该跟进落地前，PERF-EXPORT-24M 与 RESOURCE-MAX 保持
**Not tested**，#38 不声明这两项通过。

#59 已按方向 (3) 落地并在同一 Release 产品候选上重跑两项：`docs/validation/59-24m-stitch.md`。
诊断证明失配是选区贴住夹具窗口顶边时捕获到的整宽顶部重绘瞬态带，而非排除滤镜
hole。PERF-EXPORT-24M 为 **Pass**（5 次，max 789411 us）；RESOURCE-MAX 为
**Fail**（观测 peak physical footprint 3742720 KiB = 3.57 GiB > 2 GiB，约
3.1 GiB `Malloc Large` 来自 24M 拼接的全尺寸表面副本），收窄该峰值属独立后续票。
本报告的 #38 历史结果保持原样，不因 #59 改写。

## 稳定自动化子用例（Rust）

前置均为上述 SHA、锁定依赖；「实际」为对应 `cargo test` 的观察行。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| PERF-GATE-38-01 | `scripts/test-validation-tools.sh` 用 31 个 `interact_commit_us` 样本调用 `perf-gate.sh` | median=16、nearest-rank p95=30、max=31，判定 Pass | `validation_tools=pass` | Pass |
| PERF-GATE-38-02 | 3 个样本调用 `perf-gate.sh` | n<30 → Not tested | `validation_tools=pass` | Pass |
| PERF-GATE-38-03 | 30 个 `capture_request_us=200000` 样本 | p95 超门槛 → exit 1 | `validation_tools=pass`（脚本按预期拒绝） | Pass |
| PERF-GATE-38-04 | 5 个 2M / 6M / 24M `output_encode_us` 样本 | 分别落入 2M/6M/24M 桶并判定 | `validation_tools=pass` | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| PERF-INTERACT-38-01 | Release、30 次 AX 工具切换 | p95 ≤ 100 ms | n=31，median 0 ms，p95 1 ms，max 1 ms | Pass |
| PERF-REGION-38-01 | Release、31 次菜单开始截图→Esc | p95 ≤ 300 ms | n=31，median 144202 us，p95 165824 us，max 305890 us | Pass |
| PERF-SCK-38-01 | 同上 | p95 ≤ 150 ms | n=31，median 31984 us，p95 38313 us，max 125437 us | Pass |
| PERF-FRAME-PREVIEW-38-01 | Release、夹具内侧选区、30 次慢速滚轮 | p95 ≤ 200 ms，max ≤ 500 ms | n=80，median 115108 us，p95 131088 us，max 134880 us | Pass |
| PERF-EVENT-PREVIEW-38-01 | 同上 | p95 ≤ 400 ms，max ≤ 750 ms | n=79，median 149058 us，p95 283919 us，max 300866 us | Pass |
| PERF-STITCH-38-01 | 同上（1500×1360 物理接纳帧唯一位移） | p95 ≤ 100 ms | n=82，median 800 us，p95 819 us，max 898 us | Pass |
| PERF-EXPORT-2M-38-01 | Release、700×700 逻辑选区（1.96M 物理）×30 次 Finish | p95 ≤ 2 s | n=30，median 34445 us，p95 43548 us，max 61678 us | Pass |
| PERF-EXPORT-6M-38-01 | Release、1300×800 逻辑选区（4.16M 物理）×30 次 Finish | p95 ≤ 5 s | n=30，median 47034 us，p95 59241 us，max 117754 us | Pass |
| PERF-CLIPBOARD-38-01 | 同两次导出 | p95 ≤ 500 ms | n=60，median 461 us，p95 609 us，max 1104 us | Pass |
| PERF-SCROLL-START-38-01 | Release、进入滚动到“可以滚动” | p95 ≤ 1 s | n=1，422957 us（样本不足） | Not tested |
| PERF-EXPORT-24M-38-01 | Release、24M 文档连续 5 次导出 | 每次 ≤ 20 s | 内嵌预览拼接拒绝，文档未达 24M | Not tested |
| RESOURCE-MAX-38-01 | 最大导出期间 100 ms RSS + footprint | 观测 peak ≤ 2 GiB | 依赖 PERF-EXPORT-24M，未执行 | Not tested |

## Not tested

- **PERF-EXPORT-24M 与 RESOURCE-MAX**：内嵌预览拼接拒绝，见“阻塞项”。这是
  发布阻塞项，#38 不声明通过。
- **PERF-SCROLL-START 样本量**：本轮只进入一次滚动，n=1 < 30；需要 30 次独立
  进入滚动采集才能给出 p95。单样本 422957 us 远低于 1 s 门槛，但不构成达标
  证据。
- **负载敏感度**：测量期间系统 load average 在 3..41 间波动；PERF-REGION 在
  高负载组 p95 298076 us（仍 ≤ 300000 us，但余量仅 1.9 ms）。未在受控空闲
  机器上复跑。
- **原生 SCK 错误/超时实机注入、worker panic 实机复现、真实 TCC 切换、系统
  设置返回重检、运行中撤权**：属 §4.4 用户项或缺口注入，保持 Not tested。
- **视觉观感、IME 候选窗、多显示器**：属 §4.4 用户项，保持 Not tested。

## 给下一票的接缝

- 测量接缝诊断（均在 stderr，文本）：`interact_commit_us`、
  `capture_wake_window_submit_us`、`scroll_start_us`、`frame_preview_us`、
  `preview_update_us`、`event_preview_us`、`scroll_stitch_verify_us`、
  `frame_verify_us`、`output_encode_us=… pixels=WxH`、`clipboard_accept_us`、
  `capture_submit_us`、`capture_preparing_opened_us`、`capture_worker_us`、
  `capture_pre_us`、`capture_sc_content_us`、`capture_filter_us`、
  `capture_post_us`、`capture_open_window_us`、`capture_prepare_us`。
- `scripts/perf-measure.py` 每次运行清空并重写日志；若需保留原样本，请在运行后
  立即复制 `target/perf-measure-release.log`，或给 `--log` 指定独立路径。
- `perf-gate.sh` 的 nearest-rank p95 用 `ceil(0.95*n)`，与仓库既有
  `perf-samples.sh` 的 `round` 口径不同；引用时以 `perf-gate.sh` 为准。
- 准备窗口的 NSWindow number 由 `open_preparing` 从 live NSView 读取；自适应
  settle 依赖该 number 出现在 `owned_visible_window_ids()` 中。任何改动该窗口
  创建顺序的票都必须保留 `PREPARE_SETTLE` 作为绝对上限。
