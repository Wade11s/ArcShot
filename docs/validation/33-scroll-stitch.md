# #33 双向拼接、反向去重与实时已验证预览验证记录

本记录覆盖 #33 落地的 I-11 步骤 3–5 与 I-12/I-13 的相应部分：固定选区与比例下
把单帧请求循环连接成**唯一垂直位移 + 全重叠一致性 + 上下追加 + 反向去重**，
原始帧队列有界（最多 1 在途、2 待处理、满时背压不静默丢帧），固定实时预览
（右/左/选区内右下）只显示已验证内容且不入图，文档/预览/PNG 上限与 checked
尺寸运算随功能首次落地。证据来自 image 的严格拼接算法与像素真值、core 的
会话状态机、app 的有界调度与 GPUI 预览、macos 的纯门缝决策与原生适配、
自有 GPUI 精确像素夹具、真实 AX/文本诊断 E2E，以及 Debug/Release 两套完整
`scripts/check-all.sh`。

三秒完成预览、长图标注、TextEdit/Safari 目标与 #34 的显式部分结果选择全部
**未实现、未声称通过**。

## 实测来源与环境

- 基线：`spec/v1` = `f110e9e`（Merge ticket/32-scroll-single-frame）。
- 实测实现/脚本 SHA：`f28e5539b5646a92839de4b172544bad63c7f7b0`
  （`feat(scroll): bidirectional stitch, reverse dedup and live verified preview`）。
  本证据文档是后续纯文档提交，不改产品源码、脚本或测试。
- 时间：2026-11-04T21:35:48Z（Debug/Release E2E 与合并 `spec/v1` 前后的两套
  `scripts/check-all.sh` 全部完成之后）。
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
- 显示器：内建屏，1470×956 逻辑点，scale=2；`system_profiler` 报面板
  2560×1664 Retina。产品日志滚动帧
  `scroll_frame_request_us=… pixels=1160x360 stride=4640 scale=2`（选区
  580×180 逻辑点）；内侧预览 case 的选区为 760×180 逻辑点，帧为 1520×360。
  未保存屏幕图像。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 为
  `accessibility=granted`、`screen_recording=granted`、`automation=unknown`、
  `platform=macos`；被测 bundle 的预检日志为
  `scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`。
  未改动 TCC，未请求通知权限，未注册/注销登录项。
- 夹具：`fixture=scroll-single-frame-e2e-v1`（沿用 #32 的自有 GPUI 精确像素
  夹具，独立进程、无标题栏、origin=(600,120) size=760x720、
  initial_offset=2000、MAX_OFFSET=7400）。像素真值由 `fixture_pixel`
  确定性函数与 `capture_probe_grid` 5×5 逐格比对；E2E 脚本按夹具回执的
  `fixture_scroll delta_points=… offset_points=…` 计算当前偏移的期望探针。
- 日志：`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-release.log`、
  `target/e2e-scroll-fixture-debug.log`、
  `target/e2e-scroll-fixture-release.log`、`/tmp/33-e2e-debug-final.log`、
  `/tmp/33-e2e-release7.log`、`/tmp/33-check-all.log`。日志是本地构建产物，
  不入库；本报告只保存文本/数值/hash 摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本证据文档是
  后续纯文档提交。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-image stitch
cargo test --locked -p arcshot-core --test scroll_loop
cargo test --locked -p arcshot-app --test scroll_frame_tasks
scripts/check-all.sh
scripts/e2e-agent-desktop.py              # Debug
scripts/e2e-agent-desktop.py --release    # Release
```

所有原生门禁运行都在同一个 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行
执行，并在每次运行前确认 `agent-desktop list-apps --app "ArcShot"` 为空。

- `cargo clippy … -D warnings`：exit 0，无 error。
- `cargo test --locked --workspace --all-features`：exit 0，45 个测试二进制
  全部 `test result: ok`。
- `cargo test --locked -p arcshot-image stitch`：exit 0，11 passed。
- `cargo test --locked -p arcshot-core --test scroll_loop`：exit 0，23 passed。
- `cargo test --locked -p arcshot-app --test scroll_frame_tasks`：exit 0，
  11 passed。
- `scripts/check-all.sh`：exit 0；关键行：

```text
validation_tools=pass
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
privacy_smoke=pass resident_pid=71867 network_sockets=0 lsof_status=1
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示
  （非 Clippy `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=27`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release log=target/e2e-agent-desktop-release.log cases=27`。

### 链路耗时（原生 SCK 请求段）

- Debug（MAC-E2E-27 三段会话范围）：67 次 `scroll_frame_request_us`，
  最小 19,495 µs、中位 22,927 µs、最大 36,488 µs。
- Release（整轮 E2E）：180 次，最小 18,972 µs、中位 21,938 µs、最大 100,568 µs。
- 这是 SCK 单帧请求的原生耗时；拼接验证、预览缩放与 GPUI 刷新在主线程序列
  执行，未单独计时。队列满时背压停发请求，不产生额外原生请求。

### MAC-E2E-27 的原生诊断行（Debug）

```text
scroll_preview placed=right frame=[862,556,600,400]
scroll_exclusion_fixed prepared=3 collecting=2 control=81679 preview=81680
scroll_input installed=true owned_windows=2
scroll_collecting ticket=CaptureTicket(7)
scroll_stitch extended offset=32 added_rows=32 top=0 bottom=392 extensions=1 duplicates=4
scroll_stitch extended offset=128 added_rows=32 top=0 bottom=488 extensions=4 duplicates=8
scroll_stitch duplicate offset=128 top=0 bottom=488 extensions=4 duplicates=9
output_publish target=Clipboard bytes=48900
scroll_tap_command ticket=CaptureTicket(7) command=Finish

scroll_preview placed=left frame=[288,380,600,400]
scroll_exclusion_fixed prepared=3 collecting=2 control=81683 preview=81684
scroll_collecting ticket=CaptureTicket(8)
scroll_stitch extended offset=96 added_rows=32 top=0 bottom=456 extensions=3 duplicates=6
output_publish target=Clipboard bytes=70327

scroll_preview placed=inner frame=[760,160,600,400]
scroll_exclusion_fixed prepared=3 collecting=2 control=81687 preview=81688
scroll_collecting ticket=CaptureTicket(9)
scroll_preview_updated place=inner pixels=1520x360 document=1520x360 extensions=0 duplicates=0
scroll_stitch extended offset=96 added_rows=32 top=0 bottom=456 extensions=3 duplicates=5
output_publish target=Clipboard bytes=155953
```

- 右 case：4 次 `--dy -1`（每次 16 逻辑行=32 物理行）下滚，`bottom` 从 360
  逐次到 488；反向 4 次 `--dy 1` 回到初始偏移 2000，只有 `scroll_stitch
  duplicate`，扩展计数保持 4，`bottom` 保持 488。`capture_probe_grid` 在
  下滚后与反向后都等于夹具当前偏移的 5×5 像素真值。
- 左 case：3 次下滚 + 3 次反向，扩展 3 次、反向只有 duplicate。
- 内侧 case：预览窗覆盖选区（frame=[760,160,600,400]），指针位于预览上
  (1000,450) 仍产生 `fixture_scroll delta_points=-16 offset_points=2016`；
  同时 `capture_probe_grid` 与夹具当前偏移逐格相等，证明其下捕获正确；
  `scroll_exclusion_fixed … preview=81688` 证明预览窗在排除数组里，输出
  只由采集帧合成，不含浮层。
- 夹具回执（Debug `target/e2e-scroll-fixture-debug.log`）：

```text
fixture_window origin=(600,120) size=(760,720) initial_offset_points=2000
fixture_scroll delta_points=-16 offset_points=2016
…（右 case 4 下 4 上，回到 2000）
fixture_scroll delta_points=-16 offset_points=2016
…（左 case 3 下 3 上，回到 2000）
fixture_scroll delta_points=-16 offset_points=2016
…（内侧 case 3 下）
```

  整段日志没有 `fixture_click_received`；内侧 case 的滚轮发生在预览窗上，
  夹具仍收到原始滚动。

### 真实低纹理/动态目标的显式停止（MAC-E2E-24 观察）

MAC-E2E-24 的选区落在 Rex 终端上（非夹具）。采集开启后，产品按 I-11 §4
的严格验证显式停止，未产生输出：

```text
scroll_stitch_refused reason=scroll content changed inside the overlap
scroll_phase from=Collecting to=Inactive action=ScrollStitchRefused
scroll_refused reason=Stitch ContentChanged
```

此前同一位置的另一轮出现过 `reason=scroll displacement is ambiguous`
（均匀/重复内容）。两者都是已知状态停止：区域会话与选区保留，脚本按
“prepared 或已知 refusal 均可”处理。这说明低纹理与动态内容不会被猜补，
但也意味着静态纯白/重复页面会立即停止采集。

## 稳定自动化子用例（Rust）

前置均为上述 SHA、锁定依赖与注入式夹具；「实际」为对应 `cargo test` 的观察行。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| IMAGE-STITCH-61 | 原帧行 0..4；新帧行 2..6 | 唯一向下位移 2，追加 2 行，文档行 0..6 | `test stitch::tests::downward_extension_is_unique_and_contiguous … ok` | Pass |
| IMAGE-STITCH-62 | 原帧行 0..4；新帧行 -2..2 | 唯一向上位移 -2，文档行 -2..4 | `test stitch::tests::upward_extension_is_unique_and_contiguous … ok` | Pass |
| IMAGE-STITCH-63 | 先下滚到 2..6，再提交 1..5，再提交 4..8 | 反向帧去重不扩展；继续前进只追加一次 | `test stitch::tests::reverse_pass_inside_history_is_deduplicated_not_reextended … ok` | Pass |
| IMAGE-STITCH-64 | 原帧后再提交同一视图 | 无位移去重，文档不变 | `test stitch::tests::identical_viewport_without_scrolling_is_a_duplicate … ok` | Pass |
| IMAGE-STITCH-65 | 均匀内容帧 | 多位移均匹配 → 歧义拒绝 | `test stitch::tests::uniform_content_is_ambiguous_and_refuses … ok` | Pass |
| IMAGE-STITCH-66 | 重叠区一行被替换 | 部分匹配 → 内容变化拒绝 | `test stitch::tests::changed_overlap_is_content_changed … ok` | Pass |
| IMAGE-STITCH-67 | 新帧与原帧无重叠（跳变） | 缺口拒绝 | `test stitch::tests::disjoint_jump_is_a_gap … ok` | Pass |
| IMAGE-STITCH-68 | 新帧几何不同 | 几何拒绝 | `test stitch::tests::geometry_change_is_refused … ok` | Pass |
| IMAGE-STITCH-69 | 1 列、高 16384 的原帧；唯一向上扩展将超过边长上限 | 分配前 ResourceLimit，文档不变 | `test stitch::tests::extension_past_the_edge_limit_is_refused_before_allocation … ok` | Pass |
| IMAGE-STITCH-70 | 4×10 文档，预览上限 25 像素 | 确定性最近邻缩小到 2×5 且采样正确 | `test stitch::tests::preview_downscales_deterministically_within_the_pixel_cap … ok` | Pass |
| IMAGE-STITCH-71 | 文档已在上限内 | 预览保持原生尺寸 | `test stitch::tests::preview_keeps_native_size_when_already_within_cap … ok` | Pass |
| CORE-SESSION-61 | Collecting；提交一个原始帧 | 先返回 VerifyScrollFrame，完成文档仍为原帧 | `test core_session_61_collected_frame_is_held_for_verification … ok` | Pass |
| CORE-SESSION-62 | Collecting；验证扩展为 200×80 文档 | 完成 crop 变为 200×80，预览刷新 | `test core_session_62_verified_extension_replaces_the_completion_document … ok` | Pass |
| CORE-SESSION-63 | Collecting；验证反向通过 | 完成 crop 不变 | `test core_session_63_reverse_pass_does_not_change_the_completion_document … ok` | Pass |
| CORE-SESSION-64 | Collecting；拼接拒绝 Gap | 停止不输出，区域会话/选区保留，完成不可达 | `test core_session_64_stitch_refusal_stops_collection_without_output … ok` | Pass |
| CORE-SESSION-65 | Collecting；注入 InvalidFrame/ResourceLimit/WorkerUnavailable/NativeUncertain/WorkerPanicked/DisplayInvalid | 已知停止、不确定终态禁重启、内屏失效取消整会话 | `test core_session_65_collected_capture_failures_classify_like_the_initial_frame … ok` | Pass |
| CORE-SESSION-66 | 取消后迟到帧/验证/拒绝 | 全部忽略，区域会话保留 | `test core_session_66_frames_after_cancel_or_completion_are_ignored … ok` | Pass |
| APP-ASYNC-17 | 队列连推 3 帧 | 前 2 帧入队，第 3 帧 Backpressure，不丢帧 | `test app_async_17_raw_frame_queue_caps_at_two_and_reports_backpressure … ok` | Pass |
| APP-ASYNC-18 | 闸门/队列组合 | 仅闸门空闲且队列 <2 时可提交 | `test app_async_18_submission_policy_requires_an_idle_gate_and_short_queue … ok` | Pass |
| APP-ASYNC-19 | 取消清空队列 | 清空后不过期丢新会话 | `test app_async_19_cancel_clears_the_queue_without_dropping_a_new_collection … ok` | Pass |
| APP-ASYNC-20 | 闸门+队列联合策略 | 最多 1 在途 + 2 队列，背压只在消费后释放 | `test app_async_20_gate_plus_queue_allow_at_most_three_outstanding_frames … ok` | Pass |
| MAC-SCROLL-INPUT-32-01～06 | 纯门缝（回归） | 选区内原始滚动放行、点击拦截、Enter/Esc 归属、drain、重开、单意图、未跟踪按钮 | `crates/macos/src/scroll_input.rs` 单元测试全部 ok | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-26（回归） | 自有夹具单帧会话；`mouse-click`/`mouse-wheel`/`press return`/`press escape` | 首帧像素真值逐格相等；单击拦截；原始滚动放行；Enter 完成复制；Esc 返回原选区 | `mac_e2e_26=pass … pixel_oracle=exact click_blocked=true scroll_passed=true enter=clipboard scrolls=1 clicks_blocked=1`；`mac_e2e_26_esc=pass abandon=true selection_returned=true`；Debug/Release 各 `cases=27` | Pass |
| MAC-E2E-27 右 | 选区内滚动 4 下 4 上后 Enter | placed=right；扩展 4 次；反向只 duplicate；探针与当前偏移真值相等；输出 Clipboard | `mac_e2e_27_right=pass placed=right extended=true reverse_dedup=true duplicates=26 output=clipboard` | Pass |
| MAC-E2E-27 左 | 选区内滚动 3 下 3 上后 Enter | placed=left；扩展 3 次；反向 duplicate；输出 Clipboard | `mac_e2e_27_left=pass placed=left extended=true reverse_dedup=true output=clipboard` | Pass |
| MAC-E2E-27 内 | 选区内右下预览；指针在预览上滚动 3 次后 Enter | placed=inner；预览在排除集；其下捕获探针逐格相等；夹具收到原始滚动；输出 Clipboard | `mac_e2e_27_inner=pass placed=inner over_preview_scroll=true pixel_oracle=exact preview_excluded=true output=clipboard` | Pass |

Release E2E 的三个 27 子用例同样通过（`mac_e2e_27_right/left/inner=pass`，
`cases=27`）。

## Not tested

- **三秒完成预览与长图标注**：I-11 步骤 7/8 明确不在本票范围，未实现。
- **显式部分结果恢复（#34）**：拼接拒绝/资源超限后只保留已验证结果于内存
  builder 与会话状态，未提供“使用已验证部分/放弃”的用户选择；本票只显式
  停止且不自动输出。
- **TextEdit 固定文档与 Safari `file://` 静态页目标**：I-13 要求三类目标
  端到端，本票只在自有 GPUI 夹具（类 1）验证，另两类保持 **Not tested**。
- **真实 TCC 切换、System Settings 返回重检、运行中撤权**：本机三项权限均已
  授权，改动 TCC 需用户参与（TEST.md §4.4）。三权限缺失通知分支保持
  **Not tested**。
- **真实物理双击完成与全局热键在采集中的重启**：注入式门缝
  （MAC-SCROLL-INPUT-32-03、APP-EFFECT-42/44、CORE-SESSION-56）覆盖决策
  逻辑；本轮 E2E 只投递单击、滚轮与 Return/Escape，真实双击与 `⌘⇧X` 重启
  保持 **Not tested**。
- **原生请求错误/超时的实机注入、worker panic 实机复现、24M 像素/2 GiB 峰值、
  长会话性能**：只有注入式 APP-ASYNC-14/15、IMAGE-STITCH-69 与 CORE-COORD
  接缝覆盖；实机故障注入与资源峰值保持 **Not tested**。本记录中的耗时只覆盖
  单帧原生请求段。
- **2 帧队列满时的实机时序**：结构上由 APP-ASYNC-17/20 与产品策略保证；
  实机连续滚动期间未人为制造队列满（采集速率受单一在途闸门限制）。
- **IME 候选窗、多显示器、视觉观感**（控制窗/预览排版、缩略图审美）：属
  §4.4 用户项，保持 **Not tested**。
- **TextEdit/Safari 的选区内预览**：本票只验证自有夹具的三个位置。

## 给下一票（#34）的接缝

- 完成文档来自 `ScrollPreparation::stitch_frame`（初始为可信原帧，之后只由
  严格验证的扩展替换）。`complete_scroll` 把它包装成整帧选择的
  `Document` 并复用 `begin_output`/`ComposeOutput`/`publish_output`；版本
  栅栏、PNGf/保存字节一致性与“同文档 1 个合成/编码任务”都未改动。
- 拼接验证在 `arcshot_image::stitch::StitchBuilder`：唯一位移、全重叠
  一致性、上下追加、反向去重、几何/歧义/变化/缺口/资源拒绝都在这里；
  文档与预览上限分别为 `MAX_PIXELS`/`MAX_EDGE` 与 `MAX_PREVIEW_PIXELS`。
- 拒绝路径统一为 `ScrollRefusal::Stitch(ScrollStitchError)` →
  `CaptureEffect::ScrollRefused` → `PresentFrozen`，不产生输出；已验证的
  文档仍在 app 的 `scroll_stitch` builder 中。#34 的部分结果选择可以复用
  这条路径与 builder 的 `current_frame()`，但必须由用户显式选择后才输出。
- 原始帧队列在 `arcshot_app::scroll_frame_tasks::{ScrollFrameQueue,
  may_submit_scroll_frame}`：容量 2，满时返回 `Backpressure` 并由产品停止
  提交，绝不静默丢帧。#34 不得把背压当作猜补授权。
- 预览窗在准备阶段一次性打开（`scroll_preview` + `scroll_preview_id`），
  原生 `set_scroll_preview_click_through` 忽略鼠标事件；窗口集合固定为
  {控制窗, 预览窗}，采集中只替换 `ScrollPreviewView` 的 `RenderImage`。
  `install_scroll_tap` 现在要求可见自有窗口恰好是这两者；任何额外窗口拒绝
  启动。新增 UI 窗口必须先加入固定排除集并在准备阶段打开。
- `scroll_tap_command ticket=… command=Finish/Cancel` 与
  `scroll_stitch extended/duplicate/refused`、`scroll_preview placed=…`、
  `scroll_preview_updated …`、`scroll_exclusion_fixed … preview=…` 是文本
  观察面，下一票可直接复用。
