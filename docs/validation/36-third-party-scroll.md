# #36 TextEdit / Safari 受控滚动兼容验证记录

本记录覆盖 #36 的目标兼容纵切：在两个非 ArcShot 真实目标上完成受控双向
Scroll Capture 与复制，并取得目标身份 / 前台 / 公开 hit-test、选区内实时
预览、慢速双向滚动与反向去重、无滚动单帧、Enter / 双击完成、Esc 取消、
单击拦截、原始滚动到达目标、点击受保护、命名 sRGB 显示滤波全链路接纳与
内存 PNG 解码证据。证据来自 `crates/image` 的唯一位移验证回归、
`scripts/e2e-agent-desktop.py` 的 MAC-E2E-30/31（Debug 与 Release），
以及 Debug/Release 两套完整 `scripts/check-all.sh`。

本票只覆盖目标兼容纵切。完整成功预览 / 长图编辑组合由最终回归 #37 验证，
不在本记录内。两个非 ArcShot 目标均为离线受控目标：TextEdit 固定几何
静态纯文本文档，Safari `file://` 本地静态页（无远端资源、未启本地 HTTP
服务）。通过不外推为其他应用或动态内容兼容。

## 实测来源与环境

- 基线：`spec/v1` = `1c5458b`（Merge ticket/35-long-image）；本票从
  `28d9903` 分出后将该 tip 合并入分支，冲突按双方意图解决（#35 的完成
  预览后处理与 #36 的第三方目标 E2E 保留）。
- 实测实现/脚本 SHA：`aaa8e1a8fe05f4c2bb528439dc46e781650153ce`
  （`feat(scroll): verify third-party TextEdit/Safari targets end to end`）。
  本证据文档是后续纯文档提交，不改产品源码、脚本或测试。
- 时间：2026-11-05T02:29:26Z（Debug/Release E2E 与 `scripts/check-all.sh`
  全部完成之后）。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；两者均由 `scripts/build-app.sh`
  重建并自签名（身份 `ArcShot Dev`）。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1
  (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)，
  `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk`。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，
  Target arm64-apple-macosx27.2.0。
- Metal/AIR-LLD：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏，1470×956 逻辑点，scale=2；`agent-desktop list-displays`
  报 bounds `1470x956`、scale `2.0`。产品日志完成帧
  `scroll_frame_request_us=… pixels=1200x600 stride=4800 scale=2`（选区
  600×300 逻辑点）；TextEdit 扩展后 `pixels=1200x620`，Safari 扩展后
  `pixels=1200x680`。未保存屏幕图像。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 为
  `accessibility=granted`、`screen_recording=granted`、`automation=granted`、
  `platform=macos`；被测 bundle 的预检日志为
  `scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`。
  未改动 TCC，未请求通知权限，未注册/注销登录项。
- 夹具：`fixture=textedit-36-v1`（400 行编号纯文本，行内 200 个确定性
  hash 字符填满选区宽度）与 `fixture=safari-36-v1`（400 行编号静态页，
  仅内联样式，`http://`/`https://`/`<script>`/`<iframe>`/`<img>`/`src=`
  全部缺席）。两个夹具由脚本在 `target/` 下生成，不入库。
- 日志：`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-release.log`、`/tmp/check-all-36.log`、
  `/tmp/e2e36-debug2.log`、`/tmp/e2e36-release.log`。日志是本地构建产物，
  不入库；本报告只保存文本/数值/hash 摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本证据文档是
  后续纯文档提交。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-image --lib
scripts/check-all.sh                      # Debug + Release startup smoke
scripts/e2e-agent-desktop.py              # Debug
scripts/e2e-agent-desktop.py --release    # Release
```

所有原生门禁运行都在同一个 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行
执行，并在每次运行前确认 `agent-desktop list-apps --app "ArcShot"` 为空。

- `cargo clippy … -D warnings`：exit 0，无 error。
- `cargo test --locked --workspace --all-features`：exit 0，47 个测试二进制
  全部 `test result: ok`（合计 308 passed）。
- `cargo test --locked -p arcshot-image --lib`：exit 0，12 passed（含本票
  新增的边缘空白行回归）。
- `scripts/check-all.sh`：exit 0；关键行：

```text
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
privacy_smoke=pass resident_pid=43906 network_sockets=0 lsof_status=1
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示
  （非 Clippy `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=30`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release log=target/e2e-agent-desktop-release.log cases=30`。

### 修复的实际缺陷：唯一位移验证的边缘空白行误判

`StitchBuilder::submit` 原来把候选位移范围内**任意**重叠行的精确匹配都当作
一个候选。真实文本视口的顶/底边缘常有空白行；一次正常扩展后，新帧顶部的
空白行会与历史底部的空白行在 1–2 行的重叠上精确重合，于是唯一的大重叠
（TextEdit 480 行、Safari 120 行）被误判为「多个位移均解释整个重叠」→
`Ambiguous`，采集停止。修复引入最小可靠重叠 `viewport / 4`（至少 1 行）：
重叠小于该比例的候选不足以可靠确定位移，不参与唯一性判定。该规则与
`prototype/manual-scroll` 验证过的 ±3/4 视口位移上限等价（重叠 ≥1/4 视口），
并保留既有均匀内容歧义回归。

修复前 TextEdit 实测（Debug，`--dy -6` 单步）：

```text
scroll_stitch extended offset=120 added_rows=120 top=0 bottom=720 extensions=1 duplicates=4
DEBUG ambiguous candidates=[120, 718, 719]
scroll_stitch_refused reason=scroll displacement is ambiguous
```

修复后同一场景：

```text
scroll_stitch extended offset=120 added_rows=120 top=0 bottom=720 extensions=1 duplicates=4
```

（`DEBUG` 行是定位缺陷时的临时诊断，已从产品源码移除；候选 `[120, 718, 719]`
中 718/719 即 2/1 行的边缘空白行重合。）

### MAC-E2E-30（TextEdit）原生诊断行（Debug）

```text
scroll_preview placed=inner frame=[400,217,600,400]
scroll_target window_id=85581 owner=com.apple.TextEdit frame=[200,120,1000,700]
scroll_exclusion_fixed prepared=3 collecting=2 control=85589 preview=85590
scroll_stitch extended offset=20 added_rows=20 top=0 bottom=620 extensions=1 duplicates=16
scroll_stitch duplicate offset=20 top=0 bottom=620 extensions=1 duplicates=17
scroll_tap_command ticket=CaptureTicket(14) command=Finish
output_publish target=Clipboard bytes=133742 pixels=1200x620 annotations=0
completion_preview shown ticket=CaptureTicket(14) frame=[1034,723,420,217] pixels=1200x620
scroll_input_retired ticket=CaptureTicket(14) scrolls=2 clicks_blocked=1 keys_owned=1 drain_forced=false
completion_preview_expired ticket=CaptureTicket(14)
capture_phase from=Frozen to=Resident action=CancelAttempt
```

- 慢速下滚 1 行扩展 20 物理行（`bottom` 600→620）；上滚 1 行走
  `scroll_stitch duplicate`，`extensions` 保持 1，`bottom` 不变。
- 采集期单击被拦截：`clicks_blocked=1`，TextEdit 垂直滚动条值不变。
- 无滚动单帧 Enter：`output_publish … pixels=1200x600`，`scrolls=0`。
- 双击（滚动区）完成：`scroll_tap_command … command=Finish`，
  `output_publish … pixels=1200x600`，`clicks_blocked=2`（首击被吞、次击完成）。
- Esc 取消：`scroll_phase from=Collecting to=Inactive action=CancelScroll`，
  无 `output_publish`，原选区返回（`arcshot.overlay.save` 重新可寻址）。

### MAC-E2E-31（Safari）原生诊断行（Debug）

```text
scroll_preview placed=inner frame=[300,217,600,400]
scroll_target window_id=85618 owner=com.apple.Safari frame=[100,80,1200,800]
scroll_exclusion_fixed prepared=3 collecting=2 control=85626 preview=85627
scroll_stitch extended offset=80 added_rows=80 top=0 bottom=680 extensions=1 duplicates=17
scroll_stitch duplicate offset=80 top=0 bottom=680 extensions=1 duplicates=18
scroll_tap_command ticket=CaptureTicket(18) command=Finish
output_publish target=Clipboard bytes=118127 pixels=1200x680 annotations=0
completion_preview shown ticket=CaptureTicket(18) frame=[1034,702,420,238] pixels=1200x680
scroll_input_retired ticket=CaptureTicket(18) scrolls=2 clicks_blocked=1 keys_owned=1 drain_forced=false
completion_preview_expired ticket=CaptureTicket(18)
capture_phase from=Frozen to=Resident action=CancelAttempt
```

- Safari 地址栏全程为
  `file:///Users/hwj/Desktop/SI/ArcShot/.worktrees/36-third-party-scroll/target/e2e-36-safari.html`，
  单击被拦截后不变；前台为 Safari。
- 慢速下滚 1 行扩展 80 物理行（`bottom` 600→680）；上滚 1 行去重。
- 无滚动单帧 Enter、双击（滚动区）完成、Esc 取消与 TextEdit 同形。

### 内存 PNG 解码（两目标、两配置）

`decode_clipboard_expect` 经 `osascript` 读取剪贴板 `PNGf` 并在内存解码：
校验 IHDR 位深/颜色（8/6）、`sRGB` 块存在、alpha 全 255、尺寸等于选区物理
宽、行纹理非单一。Debug 单帧 SHA-256 前缀 TextEdit `1748d3ecb4d4`、
Safari `7e4c1f787f4f`；Release 单帧 TextEdit `2bb3a6d8e9ff`、Safari
`7e4c1f787f4f`。扩展帧高度等于已验证底（TextEdit 620、Safari 680）。

## 稳定自动化子用例（Rust）

前置均为上述 SHA、锁定依赖；「实际」为对应 `cargo test` 的观察行。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| IMAGE-STITCH-72 | 8 行视口，原帧第 3、8 行为空白；新帧为原帧下移 2 行且顶部行为空白 | 唯一大重叠扩展 2 行；顶部 1 行空白与历史底部空白的重合不构成第二位移 | `test stitch::tests::tiny_uniform_edge_overlap_does_not_make_a_unique_extension_ambiguous … ok` | Pass |
| IMAGE-STITCH-65（回归） | 4 行均匀内容帧 | 多位移均匹配 → 歧义拒绝 | `test stitch::tests::uniform_content_is_ambiguous_and_refuses … ok` | Pass |
| IMAGE-STITCH-64/66/67/68/69/70/71（回归） | 同视图去重、重叠一行被替换、无重叠跳变、几何变化、边长上限、预览缩小/原尺寸 | 行为不变 | `test stitch::tests::… … ok`（12 passed） | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-30 | TextEdit 固定几何纯文本（`textedit-36-v1`，System Events 固定 `200,120 1000x700`，预滚到文档中部）；开始截图后 `--headed` 拖拽建内侧选区（`400,350→1000,650`），AX 激活滚动入口；采集中点击选区、慢速滚 1 下 1 上、Enter、双击、Esc 分别验证 | `scroll_target owner=com.apple.TextEdit` 与帧 `[200,120,1000,700]`；`scroll_preview placed=inner` 且在排除集；前台为 TextEdit；点击后滚动条不变且 `clicks_blocked≥1`；下滚扩展、上滚去重；无滚动 Enter 与双击各一次 Clipboard 输出，内存 PNG 解码 `1200` 宽、sRGB、全不透明、高度等于已验证底（单帧 `600`）；Esc 放弃回原选区、无输出 | `mac_e2e_30=pass target=TextEdit scroll=extended click_blocked=true raw_scroll=true single=1200x600/sRGB/sha=1748d3ecb4d4 double_click=true esc=true fixture=textedit-36-v1`；Release `… sha=2bb3a6d8e9ff …` | Pass |
| MAC-E2E-31 | Safari `file://` 静态页（`safari-36-v1`，无远端资源，System Events 固定 `100,80 1200x800`）；同上选区（`300,350→900,650`）与流程，滚轮轴点 `600,600` | `scroll_target owner=com.apple.Safari`；内侧预览在排除集；地址栏仍为 `file://` 且前台为 Safari；点击不改地址且 `clicks_blocked≥1`；慢速双向扩展与反向去重；无滚动 Enter、双击各一次 sRGB PNG 解码（`1200×600` 单帧）；Esc 放弃回原选区 | `mac_e2e_31=pass target=Safari scroll=extended click_blocked=true raw_scroll=true single=1200x600/sRGB/sha=7e4c1f787f4f double_click=true esc=true fixture=safari-36-v1 offline=file`；Release 同形同 hash | Pass |
| MAC-E2E-26/27/28/29（回归） | 自有 GPUI 夹具单帧/多帧/部分选择/完成预览 | 既有断言不变 | `mac_e2e_26/27_right/27_left/27_inner/28_abandon/28_accept/29_refinish/29_cancel=pass`；Debug/Release 各一次 | Pass |

## Not tested

- **真机滚轮手感与物理滚轮/触控板 EventType**：本票用 `agent-desktop
  mouse-wheel` 的合成滚轮（一步一条慢行）驱动；真实滚轮的加速度、惯性、
  触控板捏合/手势与亚像素位移手感保持 **Not tested**（§4.4）。
- **快速/不连续位移的正向通过**：拼接器要求重叠 ≥1/4 视口；快速合成步进
  （如 Safari `--dy -6` 只留 120 行重叠）按无法验证安全停止，不作正向兼容
  承诺。负面通过只表示安全处理，不外推。
- **动态内容、固定表头/浮层、重复/低纹理、横向/对角/嵌套滚动**：本票只
  覆盖静态、唯一可验证的两个受控目标，其余实机负例保持 **Not tested**。
- **完整成功预览 / 长图编辑组合**：#35 的完成预览后处理与 #37 的完整回归
  覆盖；本票只断言预览出现/过期与剪贴板保留，不做标注组合。
- **非 ArcShot 应用粘贴 smoke 与显式保存字节一致**：MAC-COLOR-OUTPUT 的
  内存解码与 PNGf 回读已取证；向第三方应用实际粘贴与显式保存字节一致保持
  **Not tested**。
- **真实 TCC 切换、System Settings 返回重检、运行中撤权**：本机三项权限均
  已授权，改动 TCC 需用户参与（§4.4）。
- **原生 SCK 错误/超时实机注入、worker panic 实机复现、24M 像素 / 2 GiB
  峰值与长会话性能**：保持 **Not tested**。
- **视觉观感、IME 候选窗、多显示器**：属 §4.4 用户项，保持 **Not tested**。

## 给下一票的接缝

- 第三方目标的窗口几何与前台断言走 System Events（`set_window_geometry` /
  `frontmost_process`）；`agent-desktop list-windows` 对后台窗口返回过期界限，
  不可用于几何断言。Safari 必须 `open -a Safari <file>` 打开（裸 `open` 投
  默认浏览器）。
- 剪贴板 PNGf 经 `osascript` 内存解码（`read_clipboard_png` /
  `decode_png_metrics`）；`agent-desktop 1.1.0` 的 `clipboard-get` 在本机缺
  helper 失效。只存文本/数值/hash，不落盘截图。
- 双击完成在 TextEdit 上必须用 `scrollarea` 的 ref（`textfield` 的 ref 在
  本机报 `ACTION_FAILED: Live reads were incomplete`）；Safari 用
  `scrollarea`。
- 拼接器的最小可靠重叠在 `crates/image/src/stitch.rs` 的
  `StitchBuilder::submit`：`min_overlap = (viewport / 4).max(1)`。任何改动
  该阈值的票都必须保留 `uniform_content_is_ambiguous_and_refuses` 与
  `tiny_uniform_edge_overlap_does_not_make_a_unique_extension_ambiguous`。
- 完成预览后处理（#35）在每次成功复制后发生：E2E 必须等
  `completion_preview shown` → `completion_preview_expired` →
  `from=Frozen to=Resident action=CancelAttempt` 再开始下一会话，否则预览窗
  会污染下一帧。
