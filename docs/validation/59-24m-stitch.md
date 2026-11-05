# #59 24M 文档构建解锁与 PERF-EXPORT-24M / RESOURCE-MAX 实测

本票只做一件事：让严格逐行验证的滚动采集能构建一份 24,000,000 物理像素的
文档，并在同一 Release 产品候选上实测 #38 遗留的两个发布门槛。结论：

- **24M 文档构建：Pass**（1500×16000 物理像素，逐行拼接验证未放宽）。
- **PERF-EXPORT-24M：Pass**（连续 5 次，max 789411 us = 0.789 s ≤ 20 s）。
- **RESOURCE-MAX：Fail**（观测 peak physical footprint 3742720 KiB = 3.57 GiB，
  超过 2 GiB = 2097152 KiB；kernel `phys_footprint_peak` 同样超过）。本票不把它
  记为 Pass，也不通过降质、缩图、跳过验证或磁盘缓存规避。

## 实测来源与环境

- 实测实现/脚本 SHA：`f17ec9ccbe5c99390b1a25c6dfbcd7e3d20b990c`
  （测量入口最终状态；产品实现提交 `88392014874f7c0eeb0dcd36ee3f04428b7e765f`。
  两次提交之间 Rust 源码完全相同，只有测量脚本的采样节奏变化，因此两次门槛
  实测的是同一个 Release 产品二进制。）
- 时间：2026-11-05T11:52:44Z
- 构建配置：Release。`scripts/build-app.sh --release`，
  `target/bundle/release/ArcShot.app`（`dev.arcshot.app`，签名 `ArcShot Dev`）。
- `scripts/record-validation.sh --configuration release --logs target/perf-24m-59-perf.log`
  实测输出（逐行）：
```
commit_sha=f17ec9ccbe5c99390b1a25c6dfbcd7e3d20b990c
worktree_dirty=false
timestamp_utc=2026-11-05T11:52:44Z
build_configuration=release
machine=Mac14,2
architecture=arm64
macos=27.2 (26B123)
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
xcode=Xcode 27.0 (Build version 27A266a)
macos_sdk=27.0 (26A425)
swift=Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)
metal=Apple metal version 32023.921 (metalfe-32023.921.6)
log=target/perf-24m-59-perf.log
```
- display=1470x956 scale=2.0（`agent-desktop list-displays` 报 bounds
  `1470x956`、`scale: 2.0`，内建单屏）。
- permissions=accessibility=granted screen_recording=granted platform=macos
  （`agent-desktop status` 1.1.0 / envelope 2.6；产品预检
  `scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`；
  未改动 TCC）。
- machine=Mac14,2 / Apple M2 / 16 GB；macos=27.2 (26B123)。验收基线一致。
- fixture=`arcshot-scroll-fixture --width 800 --max-offset 7808 --initial-offset 0`
  （`crates/app/src/bin/scroll-fixture.rs`，Release 构建）。固定选区
  `(620,196)–(1370,388)` 逻辑点 = 750×192 点 = 1500×384 物理像素；
  `scroll_preview placed=left frame=[8,196,600,400]`，预览在选区外。
- log=target/perf-24m-59-perf.log,target/perf-24m-59-resource.log,target/perf-24m-59-perf.stdout,target/perf-24m-59-resource.stdout,target/perf-24m-resource-samples.log,target/perf-24m-fixture.log
  日志均为本地构建产物，不入库；本报告只保存文本/数值/hash 证据，从不保存截图。
- 所有原生门禁运行都在 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行，
  每次运行前 `agent-desktop list-apps --app "ArcShot"` 报 `apps: []`。

## 修复：解锁 24M 文档构建

诊断（在拒绝分支上一次性的文本诊断，未进入最终提交）：旧几何下
`StitchBuilder::submit` 返回 `ContentChanged`，新增的 mismatch span 报
`scroll_stitch_mismatch rows=32..79 cols=0..1499`；逐行字节显示新帧顶部
48 行是 `f6f4f4ff` / `ffffffff`（窗口背景色），已验证表面是
`e9ade3ff` / `c00fadff`（夹具确定性内容）。失配是**整宽的顶部重绘瞬态带**，
不是排除窗口 hole：它出现在选区紧贴夹具窗口顶边时。稀疏
`capture_probe_grid` 的 25 个采样点仍与夹具像素真值逐点相等，因此此前误判为
内嵌预览排除滤镜。

修复采用票面允许的第二条路径「可配置夹具几何」，不改产品采集/验证语义：

1. `arcshot-scroll-fixture` 新增 `--origin-x/--origin-y/--width/--height/
   --max-offset/--initial-offset`（默认值与所有既有 E2E 完全一致；非法参数是
   `Result` 用法错误，不 panic）。
2. 24M 构建用更宽的 800 点窗口、从内容行 0 起步、把滚动范围钳在 7808 行；
   选区从窗口顶边下移 76 点，避开顶部重绘瞬态带；预览改落在选区左侧
   （`placed=left`），固定排除窗口不再落在选区内。
3. 算术：384 + 2×7808 = 16000 物理行 × 1500 物理列 = 24,000,000 像素，
   单边 ≤ 16384。滚动到钳位后所有多余滚轮事件成为 `scroll_stitch duplicate`，
   不会触发 `Stitch ResourceLimit` 停止。
4. 逐行 hash 短路 + 逐字节复核、唯一位移、最小重叠、24M/边长/预览/编码上限
   全部保留；没有降质、缩小输出、跳过验证或磁盘缓存。
5. `image` 新增 `StitchBuilder::last_mismatch_span()`，把 `ContentChanged`
   的失配文档行与物理列范围交给 `app` 输出为 `scroll_stitch_mismatch` 文本
   诊断，方便后续定位同类边缘伪影；三个 `IMAGE-STITCH-59` 回归测试覆盖它。

`scripts/perf-measure.py` 的 24M 路径改为 5 次独立会话：产品只把已确认复制的
长图保留一次供可选标注，重新 Finish 会结束会话，因此单个会话无法产生 5 次
最大导出。每次会话重新启动夹具（内容行 0），慢速单向下滚并在
`completion_preview shown` → `completion_preview_expired` →
`from=Frozen to=Resident action=CancelAttempt` 后才开始下一会话。

## PERF-EXPORT-24M：性能运行（不启动资源采样器，T-04 要求性能/资源分开）

```sh
python3 scripts/perf-measure.py --configuration release --metrics export_24m \
  --samples 5 --runs 5 --log target/perf-24m-59-perf.log --no-build --no-sampler
grep -o "output_encode_us=[0-9]* pixels=[0-9]*x[0-9]*" target/perf-24m-59-perf.log \
  > target/perf-24m-59-gate.log
scripts/perf-gate.sh target/perf-24m-59-gate.log --min-samples 5
printf '%s\n' 789411 614857 645403 718091 662950 | sort -n | awk '...'
```

观察行（`target/perf-24m-59-perf.stdout`）：

```
  built document height=16000 (target 16000)      （5 次，逐次相同）
  export_24m run 1/5 published ... export_24m run 5/5 published
```

5 个原始样本（`output_encode_us`，`pixels=1500x16000`，单位 us）：

| 运行 | 1 | 2 | 3 | 4 | 5 |
| --- | --- | --- | --- | --- | --- |
| output_encode_us | 789411 | 614857 | 645403 | 718091 | 662950 |

- n=5；排序后 `614857, 645403, 662950, 718091, 789411`；
  median=662950 us；nearest-rank p95（`ceil(0.95×5)` = 第 5 个）=789411 us；
  max=789411 us（0.789 s）。
- 每次输出 `output_publish target=Clipboard bytes=5593285 pixels=1500x16000 annotations=0`，
  5 次 PNG 字节数一致；`clipboard_accept_us` 5 个样本
  `1529, 1491, 1415, 1494, 1510` 属另一次同源运行，此处只作参考。
- `scroll_stitch_refused` / `scroll_partial_choice available=true` 计数 = 0。
- 门槛判定观察行：
```
| PERF-EXPORT-24M | 5 | - | - | - | 20000000 (5 runs) | Pass |
perf_gate=PASS log=target/perf-24m-59-gate.log
```
- 同次全矩阵输出中 `PERF-FRAME-PREVIEW` / `PERF-EVENT-PREVIEW` 因 5 次长采集
  的 7882/7879 个样本含长尾而判 Fail；这两项由 #38 的 30 样本独立运行判定，
  本票不采集它们，子集运行的全矩阵结论记为信息性，不作为本票门槛证据。

## RESOURCE-MAX：资源运行

完整的 5 次最大导出资源运行（同一产品二进制；采样脚本为此票最终提交前的
0.1 s RSS 节奏版本）：

```sh
python3 scripts/perf-measure.py --configuration release --metrics export_24m \
  --samples 5 --runs 5 --log target/perf-24m-59-resource.log --no-build
```

`scripts/resource-footprint.sh` 同时记录 RSS、macOS `footprint` 的
`Footprint:` 当前值、kernel `phys_footprint_peak` 高水位，以及按 1 GiB
阈值打印的完整分类表。观察行（该次运行 stdout）：

```
peak_rss_kb=2844256 peak_footprint_kb=3549184 peak_phys_footprint_kb=3742720 rss_samples=7836 footprint_samples=784 footprint_total_s=50.167 elapsed_s=967 duration_s=1200
```

- 观测 peak physical footprint = **3742720 KiB = 3.57 GiB**（kernel
  `phys_footprint_peak`）；`footprint` 采样峰值 3549184 KiB = 3.38 GiB；
  peak RSS 2844256 KiB = 2.71 GiB。门槛 2 GiB = 2097152 KiB。
- 5 次会话分段（`Footprint:` 采样值）各自峰值均超 2 GiB：
  3308544 / 3093504 / 3549184 / 2925568 / 3137536 KiB；说明不是跨会话累积泄漏。
- 分类表在峰值附近（sample=1350，该次运行）给出的 native/GPU 与堆分布：
```
Footprint: 3231 MB (16384 bytes per page)
3168 MB   0 B   0 B   69   Malloc Large
  28 MB   0 B   0 B   32   IOAccelerator (graphics)
  14 MB   0 B   208 KB 12  Malloc Small
9888 KB   0 B   0 B   12   IOSurface
1056 KB   0 B   0 B   24   Owned physical footprint (unmapped) (graphics)
```
  GPU/native 侧只有几十 MiB；超预算几乎全部来自 `Malloc Large`（约 3.1 GiB，
  69 个大块区域）。编码阶段本身由 `output_encode_us` 度量（见上，约 0.6–0.8 s）。

- **Fail 原因**：构建 24M 文档要经过约 488 次严格拼接扩展，而
  `report_verified` 对每个已接纳帧调用 `StitchBuilder::current_frame()`，
  它会克隆整张已验证 BGRA 表面（最大 1500×16000×4 ≈ 91.6 MiB）；同时
  已验证拼接表面、当前文档快照、会话中的已验证帧、预览/合成副本与编码余量
  都是全尺寸副本。重复的大块分配/释放在 macOS malloc 中留下约 69 个大块
  dirty 区域，观测峰值达到 3.57 GiB，超过规格 I-13 的 2 GiB 保守预算。
  本票范围只包含「让 24M 文档可构建并实测两个门槛」，不改产品内存行为；
  收窄该峰值需要独立的后续票，不能用降质、缩图、跳过验证或磁盘缓存规避。

- 采样口径：RSS 由 `ps` 以 100 ms 节奏轮询（该次 `rss_samples=7836`），
  `footprint` 作为较慢的交叉检查（`footprint_samples=784`，
  `footprint_total_s=50.167`），并另读 kernel `phys_footprint_peak` 连续高水位。
  全部数字只称**观测峰值**，不声称覆盖所有瞬时尖峰。最终提交把 RSS 节奏收紧
  到 50 ms（观测平均间隔 <100 ms）并加入高水位与分类表输出；用该版本重跑的
  确认运行在第 3/5 次导出后被中止，**未完成**，不计入门槛判定。其
  `target/perf-24m-resource-samples.log` 仍保留 9723 个原始 RSS 样本作旁证：
  `peak_rss_kb=2831952`、`peak_footprint_kb=3257344`、
  max `phys_footprint_peak_kb=3437568` (3.28 GiB)，分类表同样显示
  `3113 MB Malloc Large` / `26 MB IOAccelerator (graphics)` / `9888 KB IOSurface`。

## 稳定自动化子用例（Rust）

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| IMAGE-STITCH-59-01 | `cargo test --locked -p arcshot-image image_stitch_59`：重叠内单列一字节变化 | `ContentChanged` 且 `last_mismatch_span` = 行 3..3、列 2..2；历史高度不变 | `test result: ok. 3 passed` | Pass |
| IMAGE-STITCH-59-02 | 同上：拒绝后成功扩展 | 成功提交清除旧 mismatch span | `test result: ok. 3 passed` | Pass |
| IMAGE-STITCH-59-03 | 同上：模拟内嵌预览下整宽顶部 48 行重绘瞬态带（128 行视口） | 严格验证必须 `ContentChanged`，span = 行 32..79、列 0..3，不扩展、不猜补 | `test result: ok. 3 passed` | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| 24M-BUILD-59-01 | Release、权限齐备、单一实例；夹具 `--width 800 --max-offset 7808 --initial-offset 0`，选区 750×192 点，慢速单向下滚 | 严格逐行验证构建 1500×16000 = 24,000,000 物理像素文档，内嵌/侧置预览不污染捕获，不产生 `scroll_stitch_refused` | `built document height=16000 (target 16000)` ×5；`scroll_preview placed=left frame=[8,196,600,400]`；`scroll_stitch_refused`=0 | Pass |
| PERF-EXPORT-24M-59-01 | 同一候选、不启动资源采样器，连续 5 次各自完整导出到剪贴板 | 5 次 `output_encode_us` 均 ≤ 20 s | 5 次：789411 / 614857 / 645403 / 718091 / 662950 us；n=5，median 662950，nearest-rank p95 789411，max 789411 us；`perf_gate=PASS` | Pass |
| RESOURCE-MAX-59-01 | 同一产品二进制、5 次最大导出期间 100 ms RSS + macOS `footprint` 交叉检查 + kernel 高水位 | 观测 peak physical footprint ≤ 2 GiB | `peak_phys_footprint_kb=3742720` (3.57 GiB)，`peak_footprint_kb=3549184`，`peak_rss_kb=2844256`；`3168 MB Malloc Large` | Fail |
| RESOURCE-MAX-59-02 | 50 ms 节奏的确认运行 | 完整 5 次导出 | 第 3/5 次导出后中止，只有 3 次 `output_publish`；未完成 | Not tested |

## Not tested

- **瞬时尖峰全覆盖**：所有峰值数字均为观测峰值（含 kernel 高水位），不声称
  覆盖采样间隔之外的每一次瞬时尖峰。
- **最终 50 ms 节奏的完整资源运行**：被中止，见 RESOURCE-MAX-59-02；其原始
  样本只作旁证，不用于门槛判定。
- **PERF-SCROLL-START 的 30 样本 p95**、PERF-INTERACT 等全矩阵指标：本票只
  采集 24M 门槛，#38 的独立 30 样本运行结果不因本票重跑而更新。
- **真实 TCC 切换/运行中撤权、SCK 错误/超时注入、worker panic 实机复现**、多显示器、
  视觉观感与动画、IME 候选窗：属 §4.4 用户项或注入缺口，保持 Not tested。
- **TextEdit / Safari 在 24M 尺寸下的端到端**：I-13 的三类目标中本票只使用自有
  精确像素夹具；不把夹具结果外推为其他应用兼容。

## 给下一票的接缝

- `scroll_stitch_mismatch rows=… cols=…` 是 `ContentChanged` 拒绝时的文本诊断，
  行是已验证文档行，列是物理像素列；`StitchBuilder::last_mismatch_span()` 由
  `image` 提供，`app` 只记录，不参与验证决策。
- 24M 夹具几何参数由 `scripts/perf-measure.py` 的 `fixture_24m_args()` 固定；
  改选区高度或 `--max-offset` 必须保持 `384 + 2×7808 = 16000` 的算术，且不得
  让选区贴住夹具窗口顶边。
- **2 GiB 超预算是独立的后续票**：`Malloc Large` 约 3.1 GiB 来自 24M 拼接过程中
  反复克隆整张已验证表面与多份全尺寸副本；`IOAccelerator`/`IOSurface` 只有几十
  MiB。任何收窄都必须保留逐行验证与 24,000,000 像素输出，不得降质、缩图或落盘。
