# #38 最大长图资源预算修复与 T-04 门槛验证记录

本记录证明 `RESOURCE-MAX` 从 **3.57 GiB**（#59 记录的观测峰值，超过 2 GiB 预算）
降到 **1.52 GiB**，以及同一候选上 T-04 的 11 项指标全部达标。它不替代 §4.4 的
用户项（真实 TCC 切换、IME 候选窗、视觉判断），也不把单档测量外推为完整发布
矩阵通过。测量过程未保存屏幕取证图像、未联网、未改动系统权限或登录项。

## 实测来源与环境

- 实测实现/脚本 SHA：`50719acd8b50dad42152d8cb36faa35af4900951`
  （`fix(stitch): grow the verified surface in place and time Scroll latency per session`，
  工作树干净）。
- 时间：2026-11-05T22:32:46Z（`scripts/record-validation.sh` 采集）。
- 构建配置：Release。`scripts/build-app.sh --release` 产出，本机自签名身份 `ArcShot Dev`。
- 入口：`scripts/perf-measure.py --no-build …`，判定 `scripts/perf-gate.sh`。
- 夹具：自有 GPUI 滚动夹具 `arcshot-scroll-fixture`（`fixture=agent-desktop-e2e-v1`）；
  24M 几何由 `fixture_24m_args()` 固定为 `--width 800 --max-offset 7808
  --initial-offset 0`，选区 750×192 逻辑点（1500×384 物理），拼接高度
  `384 + 2×7808 = 16000` 行 × 1500 列 = 24,000,000 物理像素。
- 日志：`target/perf-38-idle.log`、`target/perf-38-scroll.log`、
  `target/perf-38-scroll-start.log`、`target/perf-38-24m-perf.log`、
  `target/perf-38-24m-resource.log`、`target/perf-38-24m-resource-samples-final.log`、
  `target/38-check-all.log`、`target/38-validation.txt`。日志是本地构建产物，不入库。

## 自动记录的环境

```text
commit_sha=50719acd8b50dad42152d8cb36faa35af4900951
worktree_dirty=false
timestamp_utc=2026-11-05T22:32:46Z
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
```

- 显示器：内建单屏 1470×956 逻辑点、scale=2。
- 权限：`agent-desktop status` 报告 `accessibility=granted`、`screen_recording=granted`；
  未改动 TCC。

## 缺陷与修复

1. **拼接表面每帧重建。** `StitchBuilder::submit` 为每次扩展分配一个全新的
   全文档表面并把旧表面拷进去；`current_frame()` 又按“当前文档高度”克隆一份交给
   会话。两者都随文档增长而改尺寸，于是分配器为每一种尺寸各留一片区域：24M
   构建期间观测到 123 次“破纪录的大分配”，`Malloc Large` 达 3168 MB。
   现在表面按几何增长在原位扩展（向上扩展用一次 `copy_within`），
   `current_frame()` 按表面已有容量分配；同样的 24,000,000 像素构建只有 6 次
   破纪录分配，累计 207 MB。
2. **实时预览重复克隆。** `refresh_scroll_preview` 把已打包的预览字节
   `clone()` 一份再交给 GPUI；改为直接移交所有权。
3. **诊断时间戳跨会话。** `PERF-EVENT-PREVIEW` 的事件时间戳在取用后未标记已上报，
   同一事件会在后续每次预览刷新时被重新计时；`PERF-FRAME-PREVIEW` 与
   `PERF-REGION` 的起点在 Scroll 取消后重新显示冻结覆盖层时再次被使用。实测
   由此产生 10.7 s 的事件→预览、5.2 s 的帧→预览与 3.2 s 的 Start→可交互样本。
   现在每个会话开始时重置这些瞬时值，且每个事件只上报一次、每次捕获只上报
   一次唤醒延迟（E2E 另有断言）。

## 稳定自动化子用例（Rust）

前置为上述 SHA、锁定依赖；命令为 `cargo test --locked -p arcshot-image`。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| IMAGE-LIMIT-38-01 | `crates/image/tests/stitch_budget.rs`：合成 24,000,000 像素文档（125 次 128 行扩展，每帧按会话方式物化已验证文档）并统计破纪录大分配 | 破纪录分配 ≤12 次，累计 ≤3 倍文档字节 | 6 次、207,360,000 B（旧实现 123 次、约 3.4 GB 量级；已确认旧实现在本断言下失败） | Pass |
| IMAGE-LIMIT-38-02 | `crates/image/src/stitch.rs`：下→上→下→上交替扩展 | 内容顺序保持每行恰好一次（254,255,0,…,7 与 252…7），top=-4、bottom=8、extensions=4、added_rows=8 | `cargo test` 通过 | Pass |

`IMAGE-LIMIT-38-01` 用测试内 `#[global_allocator]` 只观察分配尺寸：比此前任何
请求都大的分配无法从空闲池复用，正是累积为常驻页的那一类。

## T-04 测量（同一候选，按场景分开运行）

性能与资源分开运行：前四组 `--no-sampler`，最后一组启动 `resource-footprint.sh`
（RSS 目标 50 ms，实测平均 71.6 ms；physical footprint 用 macOS `footprint`
交叉检查 1314 次）。单位均为微秒。

```sh
scripts/perf-measure.py --no-build --no-sampler \
  --metrics region_sck,interact,clipboard_export --log target/perf-38-idle.log
scripts/perf-measure.py --no-build --no-sampler \
  --metrics scroll --log target/perf-38-scroll.log
scripts/perf-measure.py --no-build --no-sampler \
  --metrics scroll_start --log target/perf-38-scroll-start.log
scripts/perf-measure.py --no-build --no-sampler \
  --metrics export_24m --runs 5 --log target/perf-38-24m-perf.log
scripts/perf-measure.py --no-build \
  --metrics export_24m --runs 5 --log target/perf-38-24m-resource.log
```

| 稳定指标 | 测量场景与 n | median | nearest-rank p95 | max | 门槛 | 结果 |
| --- | --- | --- | --- | --- | --- | --- |
| PERF-INTERACT | 空闲态工具切换，n=151 | 1 | 6 | 10 | ≤100000 | Pass |
| PERF-REGION | 空闲态菜单开始截图，n=92 | 113658 | 121794 | 246342 | ≤300000 | Pass |
| PERF-REGION | 滚动入口取消后的下一条捕获，n=30 | 112141 | 122019 | 239035 | ≤300000 | Pass |
| PERF-SCROLL-START | 30 次独立进入滚动 | 335498 | 356100 | 367714 | ≤1000000 | Pass |
| PERF-FRAME-PREVIEW | 普通文档慢速滚动，n=40 | 74512 | 88244 | 90636 | ≤200000，max ≤500000 | Pass |
| PERF-FRAME-PREVIEW | 24M 构建期间，n=6613 | 78432 | 94388 | 116392 | ≤200000，max ≤500000 | Pass |
| PERF-EVENT-PREVIEW | 普通文档慢速滚动，n=30 | 58211 | 132282 | 147715 | ≤400000，max ≤750000 | Pass |
| PERF-EVENT-PREVIEW | 24M 构建期间，n=2441 | 75574 | 149114 | 171846 | ≤400000，max ≤750000 | Pass |
| PERF-SCK | 常规单帧请求，n=92 | 24431 | 27539 | 112559 | ≤150000 | Pass |
| PERF-STITCH | 普通文档，n=77 | 424 | 451 | 604 | ≤100000 | Pass |
| PERF-STITCH | 24M 构建，n=11203 | 447 | 524 | 1197 | ≤100000 | Pass |
| PERF-EXPORT-2M | 700×700 逻辑选区×30 次 Finish | 10174 | 11099 | 11202 | ≤2000000 | Pass |
| PERF-EXPORT-6M | 1300×800 逻辑选区×30 次 Finish | 20628 | 21355 | 21797 | ≤5000000 | Pass |
| PERF-EXPORT-24M | 连续 5 次 24M 导出 | 597610 / 597789 / 595516 / 594349 / 599819 | — | 599819 | 每次 ≤20000000 | Pass |
| PERF-CLIPBOARD | 60 次 Finish 的剪贴板接受 | 272 | 297 | 412 | ≤500000 | Pass |

24M 导出的 PNG 为 1500×16000、5,593,285 字节（≤128 MiB），5 次完全相同。

### RESOURCE-MAX

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| RESOURCE-MAX-38-02 | 同一候选、不启动性能判定；5 次最大导出期间 50 ms 目标节奏 RSS + macOS `footprint` + kernel 高水位 | 观测 peak physical footprint ≤ 2 GiB | `peak_phys_footprint_kb=1593344`（1.52 GiB）、`peak_footprint_kb=1574912`、`peak_rss_kb=1537040`；采样 13135 次 RSS / 1314 次 footprint，历时 940 s | Pass |
| RESOURCE-MAX-38-03 | 峰值处的分类表（第一个越过 1 GiB 的采样，`sample=1350`） | 记录 native/GPU 与编码阶段分布 | `970 MB Malloc Large`、`26 MB IOAccelerator (graphics)`、`9888 KB IOSurface`、`TOTAL 1029 MB`（#59 同一位置为 3168 MB Malloc Large） | Pass |

与 #59 的对照（同一机器、同一夹具几何、同一 Release 配置）：

| 项目 | #59（`733626a` 之前） | 本记录（`50719ac`） |
| --- | --- | --- |
| peak physical footprint | 3742720 KiB = 3.57 GiB | 1593344 KiB = 1.52 GiB |
| peak RSS | 2844256 KiB | 1537040 KiB |
| Malloc Large（峰值分类表） | 3168 MB | 970 MB |
| 24M 连续 5 次导出 | Pass（789411 µs max） | Pass（599819 µs max） |

## Not tested

- **单日志全矩阵判定**：把全部场景塞进一次运行会让 `perf-gate.sh` 把不同场景的
  样本混在一起（例如滚动取消的收尾样本进入 PERF-REGION、24M 采集样本进入
  PERF-EVENT-PREVIEW）。该组合运行在修复前会因跨会话时间戳产生 10.7 s / 3.2 s
  的假长尾，修复后同一次组合运行仍会出现真实的长尾（滚动保护退役期间开始的
  捕获）。本记录按 T-04 的场景定义分别测量，并保留两份场景实测；不声称“单次
  全矩阵运行”的结果。
- **预热样本**：T-04 要求交互与预览项目“预热一次后至少 30 样本”。本记录的
  PERF-SCROLL-START 取 30 次独立进入滚动、未另设一次预热（每次进入都包含完整的
  原生准备，没有可单独预热的交互路径）；PERF-INTERACT/REGION/SCK 各 n=92～151，
  PERF-EXPORT-2M/6M 各 n=30 且未剔除首次，因此比“预热后 30 样本”更严格的一侧
  混在样本里，没有放宽。
- **T-04 之外的门槛**：`PERF-EXPORT-24M` 只测导出耗时与字节，不重测 #59 的
  逐行拼接真值；`RESOURCE-MAX` 是观测峰值，不覆盖采样间隔之外的每一次瞬时尖峰。
- **真实 TCC 切换、SCK 错误/超时注入、worker panic 实机复现、多显示器、IME
  候选窗、视觉观感**：属 §4.4 用户项或注入缺口，保持 Not tested。
- **TextEdit / Safari 在 24M 尺寸下的端到端**：本记录只用自有精确像素夹具，
  不外推为其他应用兼容。

## 给下一票的接缝

- 分配预算证据：`crates/image/tests/stitch_budget.rs` 的计数分配器；新增或改动
  拼接表面时必须保持“破纪录大分配 ≤12 次、累计 ≤3 倍文档字节”。
- 延迟诊断：`scroll_preview_updated` 之后的 `frame_preview_us` / `event_preview_us`
  只覆盖本次采集；`capture_wake_window_submit_us` 每次捕获只出现一次，
  E2E MAC-E2E-26 的 Esc 放弃分支断言 `wake_reported=1`（覆盖层二次呈现不得重新计时）。
- `scripts/perf-measure.py` 的每个 `--metrics` 组会清空目标日志；保留原始样本需在
  运行后立即复制 `target/perf-measure-release.log` 或给 `--log` 独立路径。
- 24M 采集期间仍属长会话：`scroll_stitch_verify_us` 与预览刷新的 tail 依赖
  `current_frame()` 的全文档拷贝；若后续要把这段拷贝也去掉，需同时保持
  `IMAGE-STITCH` 的严格验证与 `PERF-*` 场景定义。
