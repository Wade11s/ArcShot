# #32 受保护的单帧滚动完成闭环验证记录

本记录覆盖 #32 落地的 I-11 步骤 3/6/10 与 I-12 的采集半边：从
`ScrollPhase::Prepared` 起的 Quartz active-session 输入保护、目标前台握手、
固定自有窗口精确排除、可信初始单帧、采集状态、Enter/双击完成与 Esc 放弃，
以及 I-13 的尺寸/分配预算和失败分级。证据来自 core 语义动作、app 注入式调度、
macos 的纯门缝决策与真实原生适配、自有 GPUI 精确像素夹具、真实 AX/文本诊断
E2E，以及 Debug/Release 两套完整 `scripts/check-all.sh`。

#33 拥有的多帧拼接、唯一位移验证、反向去重、上下追加、实时已验证预览与
三秒完成预览全部**未实现、未声称通过**；本票只交付无滚动单帧完成闭环。

## 实测来源与环境

- 基线：`spec/v1` = `ec1f869`；#32 功能提交 `e8b0226`（core）、`db6504f`
  （app/macos 接线）、`11b3b7c`（产品接线、夹具、E2E、TEST.md）、
  `315c0ab`（`cargo run` 默认目标修复）。
- 实测实现/脚本 SHA：`315c0ab07866cbbf67272e2427aaa79e7bc08b19`
  （本证据文档是后续纯文档提交，不改产品源码、脚本或测试）。
- 时间：2026-11-04T22:02:11Z（`scripts/check-all.sh` 合并后完成）；
  Debug E2E 与 Release E2E 分别在 2026-11-04 完成，两次完整入口也分别完成。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；两者均由 `scripts/build-app.sh`
  重建，本机自签名身份 `ArcShot Dev`（`D86592B863E6AFA3530B59F2DC9CF170EA77CB1C`）。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)，
  `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk`。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，
  Target arm64-apple-macosx27.2.0。
- Metal/AIR-LLD：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏 ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；产品日志
  `capture_request_us=… display=1 pixels=2940x1912 stride=11760 scale=2` 给出物理
  2940×1912；`system_profiler` 报面板 2560×1664 Retina。滚动帧日志
  `scroll_frame_request_us=… pixels=1160x360 stride=4640 scale=2`（选区
  580×180 逻辑点）。未保存屏幕图像。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 为
  `accessibility=granted`、`screen_recording=granted`、`automation=unknown`、
  `platform=macos`；被测 bundle 的三权限预检日志为
  `scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`。
  未改动 TCC，未请求通知权限，未注册/注销登录项。
- 夹具：`fixture=scroll-single-frame-e2e-v1`：`arcshot-scroll-fixture`（自有 GPUI
  精确像素目标，独立进程、无标题栏、固定 origin=(600,120) size=760x720、
  initial_offset=2000）、`agent-desktop 1.1.0`、core/app 注入式调度夹具、
  真实 `NSUserDefaults` 偏好域与真实 `SMAppService` 只读状态。无外部图像夹具；
  像素真值由 `fixture_pixel` 确定性函数与 `capture_probe_grid` 5×5 逐格比对。
- 日志：`target/e2e-agent-desktop-debug.log`、`target/e2e-agent-desktop-release.log`、
  `target/e2e-scroll-fixture-debug.log`、`target/e2e-scroll-fixture-release.log`、
  `/tmp/32-check-all-3.log`、`/tmp/32-check-all-after-merge.log`、
  `/tmp/32-e2e-debug-final4.log`、`/tmp/32-e2e-release-final.log`。日志是本地
  构建产物，不入库；本报告只保存文本/数值/hash 摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本证据文档是后续
  纯文档提交。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test scroll_loop
cargo test --locked -p arcshot-app --test scroll_frame_tasks --test scroll_input_policy
cargo test --locked -p arcshot-macos --lib
cargo doc --locked --workspace --no-deps
scripts/check-all.sh                      # 合并前
git merge spec/v1                         # Already up to date.
scripts/check-all.sh                      # 合并后
scripts/e2e-agent-desktop.py              # Debug
scripts/e2e-agent-desktop.py --release    # Release
```

所有原生门禁运行都在同一个 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行
执行，并在每次运行前确认 `agent-desktop list-apps --app "ArcShot"` 为空。

- `cargo clippy … -D warnings`：exit 0，无 error。
- `cargo test --locked --workspace --all-features`：exit 0，45 个测试二进制
  全部 `test result: ok`。
- `scripts/check-all.sh`（合并前与合并后各一次）：exit 0；关键行：

```text
validation_tools=pass
validation_reports=pass directory=docs/validation
dependency_inspection=pass
direct[arcshot-app]=anyhow 1.0.104, arcshot-core 0.1.0, arcshot-image 0.1.0, arcshot-macos 0.1.0, gpui-pre 0.3.11, gpui-pre-platform 0.3.11, image 0.25.10, log 0.4.34, raw-window-handle 0.6.2
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
privacy_smoke=pass resident_pid=49568 network_sockets=0 lsof_status=1
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示
  （非 Clippy `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=26`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release log=target/e2e-agent-desktop-release.log cases=26`。

### MAC-E2E-26 的原生诊断行（Debug / Release 同形）

```text
level=INFO target=arcshot::coordinator scroll_target window_id=73544 owner=arcshot-scroll-fixture frame=[600,120,760,720]
level=INFO target=arcshot::coordinator scroll_control_window number=Some(73636)
level=INFO target=arcshot::coordinator scroll_exclusion_fixed prepared=2 collecting=1 control=73636
level=INFO target=arcshot::coordinator scroll_input installed=true owned_windows=1
level=INFO target=arcshot::coordinator scroll_frame_request_us=22262 pixels=1160x360 stride=4640 scale=2
level=INFO target=arcshot::coordinator capture_probe_grid rows=ad11baff 08bc3eff 63f0ebff f716f5ff 061dc1ff | …
level=INFO target=arcshot::coordinator scroll_phase from=Prepared to=Collecting action=ScrollFrameAdmitted
level=INFO target=arcshot::coordinator scroll_collecting ticket=CaptureTicket(5)
level=INFO target=arcshot::coordinator output_publish target=Clipboard bytes=96254
level=INFO target=arcshot::coordinator scroll_input_retired ticket=CaptureTicket(5) scrolls=1 clicks_blocked=1 keys_owned=1 drain_forced=false
level=INFO target=arcshot::coordinator scroll_phase from=Collecting to=Inactive action=CancelScroll
level=INFO target=arcshot::coordinator scroll_input_retired ticket=CaptureTicket(6) scrolls=0 clicks_blocked=0 keys_owned=1 drain_forced=false
fixture_window origin=(600,120) size=(760,720) initial_offset_points=2000
fixture_key_received key="a"
fixture_scroll delta_points=-96 offset_points=2096
```

`capture_probe_grid` 与 `expected_fixture_probe(selection)` 逐格相等；夹具日志中
没有 `fixture_click_received`（单击被吞），有 `fixture_key_received`（非自有键
放行）与 `fixture_scroll`（选区内原始滚动放行）。状态 AX 节点
`arcshot.scroll.status` 值为“可以滚动”，`arcshot.scroll.hint` 暴露操作提示；
没有百分比、ETA 或暂停按钮。Enter 后 `output_publish target=Clipboard`，
Esc 后 `arcshot.overlay.save` 重新可寻址（返回进入前选区）。

## 稳定自动化子用例（Rust）

前置均为上述 SHA、锁定依赖与注入式夹具；「实际」为
`cargo test --locked --workspace --all-features` 的观察行。全部 **Pass**。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-SESSION-46 | 已 Prepared；报告输入保护安装成功 | 产生 `CaptureScrollFrame(ticket)`，相位仍 Prepared | `test core_session_46_installed_input_requests_the_trusted_first_frame … ok` | Pass |
| CORE-SESSION-47 | 已 Prepared；注入 NotFrontmost/TapUnavailable/GestureInProgress/StillDraining | 四种已知失败都 `ScrollRefused + PresentFrozen`，保留区域会话 | `test core_session_47_input_install_failures_refuse_and_keep_the_region_session … ok` | Pass |
| CORE-SESSION-48 | 已 Prepared 且已安装输入；注入与选区几何一致的可信帧 | 相位→Collecting，区域文档与选区不变 | `test core_session_48_trusted_first_frame_opens_collection … ok` | Pass |
| CORE-SESSION-49 | 已安装输入；注入 InvalidFrame/ResourceLimit/WorkerUnavailable/NativeUncertain/WorkerPanicked/DisplayInvalid | 已知失败拒绝；不确定终态禁重启；内屏失效取消整会话 | `test core_session_49_frame_failures_classify_into_refusal_terminal_or_session_cancel … ok` | Pass |
| CORE-SESSION-50 | 已安装输入；注入全屏尺寸帧 | 几何不符拒绝，不污染文档 | `test core_session_50_admitted_frame_must_match_the_fixed_selection_geometry … ok` | Pass |
| CORE-SESSION-51 | 已 Collecting，无任何滚动；`CompleteScroll` | 提交一个 Clipboard `ComposeOutput`，crop 为整帧 60×40；输出期禁重复/重启 | `test core_session_51_enter_completes_the_single_frame_without_any_scrolling … ok` | Pass |
| CORE-SESSION-52 | 已完成编码；`OutputCompleted Ok(())` | 结束整会话；音效开时发声、关时静默 | `test core_session_52_successful_completion_ends_the_session_and_respects_the_sound_preference … ok` | Pass |
| CORE-SESSION-53 | 已完成编码；注入剪贴板失败 | 报告失败、保留采集与版本，Enter 可重试 | `test core_session_53_failed_completion_keeps_collection_and_allows_retry … ok` | Pass |
| CORE-SESSION-54 | 已 Collecting；`CancelScroll` | 相位→Inactive；同一冻结文档、同一版本、同一选区返回 | `test core_session_54_esc_abandons_the_scroll_result_and_returns_the_pre_scroll_selection … ok` | Pass |
| CORE-SESSION-55 | 完成输出在途时 `CancelScroll` | 发布栅栏失效，迟到的成功结果不复制、不发声 | `test core_session_55_esc_while_output_runs_cancels_the_publish_fence … ok` | Pass |
| CORE-SESSION-56 | 已 Collecting 且输出空闲/在途；`Start` | 空闲时放弃采集并重开；输出在途时忽略热键 | `test core_session_56_hotkey_restart_abandons_collection_but_waits_for_output … ok` | Pass |
| CORE-SESSION-57 | 已 Collecting；`ScrollInputLost` | 显式 Input(ProtectionLost) 停止，无输出，保留区域会话 | `test core_session_57_input_protection_loss_stops_collection_without_output … ok` | Pass |
| CORE-SESSION-58 | 已 Collecting；注入 NoTarget/NotFrontmost/Geometry | 三种目标失效都停止并保留选区 | `test core_session_58_target_loss_during_collection_stops_without_output … ok` | Pass |
| CORE-SESSION-59 | 取消后迟到的输入/帧/完成/失效动作；再次 EnterScroll | 全部忽略；新尝试从 CheckingPermissions 开始 | `test core_session_59_stale_scroll_actions_never_affect_another_attempt … ok` | Pass |
| CORE-SESSION-60 | 已 Collecting；再次注入帧 | 单帧环忽略第二帧（不静默排队），原完成仍只一次 | `test core_session_60_duplicate_frame_admission_is_ignored_not_queued … ok` | Pass |
| CORE-COORD-10 | display+crop=(20,10,60,40) 与最大预算 crop | 唯一换算：origin=(10,5)、pixels=(60,40)、scale=2；24M/16,384 预算前置 | `test core_coord_10_scroll_geometry_is_the_single_selection_conversion … ok` | Pass |
| APP-ASYNC-13 | `ScrollFrameTasks` 两次 submit；poll 得到结果后再 submit | 单在途；Busy；已知结果后闸门重开 | `test app_async_13_single_in_flight_gate_reopens_only_after_a_known_result … ok` | Pass |
| APP-ASYNC-14 | 阻塞 worker 越过 5 秒绝对期限 | `NativeUncertain` 终态，禁止再次提交 | `test app_async_14_absolute_deadline_is_terminal_and_forbids_resubmission … ok` | Pass |
| APP-ASYNC-15 | worker panic | `WorkerPanicked` 终态，禁止会话内重启 | `test app_async_15_worker_panic_is_terminal_native_uncertainty … ok` | Pass |
| APP-ASYNC-16 | 取消后迟到的帧结果 | 交给版本栅栏，不静默丢弃 | `test app_async_16_a_late_result_after_cancel_is_delivered_not_dropped_silently … ok` | Pass |
| APP-FRAME-03 | 合法命名 sRGB BGRA 帧 | 接纳且几何等于固定选区换算 | `test app_frame_03_valid_native_capture_admits_with_the_fixed_selection_geometry … ok` | Pass |
| APP-FRAME-04 | 宽高不符 / stride 过短 / 字节数不符 | 三种畸形都在接纳前 `InvalidFrame` | `test app_frame_04_scroll_admission_rejects_malformed_dimensions_stride_and_bytes … ok` | Pass |
| APP-FRAME-05 | 一个 alpha=254 像素 | 全不透明契约拒绝 | `test app_frame_05_scroll_admission_requires_full_opacity_under_the_selection_geometry … ok` | Pass |
| APP-EFFECT-42 | Collecting/非 Collecting、输出忙/闲时注入 Finish | 仅 Collecting 且输出空闲路由 `CompleteScroll` | `test app_effect_42_finish_routes_only_from_collecting_and_an_idle_output … ok` | Pass |
| APP-EFFECT-43 | 五个活动相位（含输出期）注入 Cancel | 全部路由 `CancelScroll`；Inactive 不路由 | `test app_effect_43_cancel_routes_from_every_active_scroll_phase … ok` | Pass |
| APP-EFFECT-44 | 混合相位/命令矩阵 | 只带当前 ticket，不臆造相位 | `test app_effect_44_actions_carry_the_live_ticket_and_never_invent_a_phase … ok` | Pass |
| MAC-SCROLL-INPUT-32-01 | 纯门缝：Armed/Collecting，选区内/外 scroll | 仅开采集后选区内原始滚动 Pass（原事件，不转发） | `test scroll_input::tests::gate_passes_raw_scroll_only_inside_the_open_selection … ok` | Pass |
| MAC-SCROLL-INPUT-32-02 | 左/右/其他按钮 down-drag-up | 全部 Swallow；未拥有的 up 才 Pass | `test scroll_input::tests::gate_blocks_clicks_and_owns_their_releases … ok` | Pass |
| MAC-SCROLL-INPUT-32-03 | 双击首击 click_state=1、次击=2；控制窗上双击 | 次击完成、首击不泄漏；控制窗双击归 UI | `test scroll_input::tests::gate_double_click_completes_from_the_second_click … ok` | Pass |
| MAC-SCROLL-INPUT-32-04 | Enter/小键盘 Enter/Esc 与其它键 | ArcShot 拥有按下释放归属；其它键与热键放行 | `test scroll_input::tests::gate_owns_enter_and_escape_with_matched_releases … ok` | Pass |
| MAC-LIFETIME-PRIVACY-32-01 | 采集结束进入 drain，持有自有鼠标按下 | 新输入放行、自有释放仍吞，全部释放后 drained | `test scroll_input::tests::drain_keeps_release_only_interception_until_owned_input_releases … ok` | Pass |
| MAC-LIFETIME-PRIVACY-32-02 | 命令键按下产生意图后进入 drain | 已消费的意图键不阻塞 tap 清理 | `test scroll_input::tests::consumed_command_key_does_not_block_the_drain … ok` | Pass |
| MAC-TARGET-FAIL-32-01 | 系统禁用 tap | 显式 ProtectionLost，不当成成功 drain | `test scroll_input::tests::tap_disabled_by_the_system_is_lost_protection … ok` | Pass |
| MAC-SCROLL-INPUT-32-05 | 失败完成后 `reopen_collection` | 滚动与 Enter 恢复，无需重装 tap | `test scroll_input::tests::gate_reopens_collection_after_a_failed_completion … ok` | Pass |
| MAC-SCROLL-INPUT-32-06 | 同一轮询内多个意图 | 只排第一个，不覆盖 | `test scroll_input::tests::gate_queues_only_the_first_intent … ok` | Pass |
| MAC-NATIVE-FAIL-32-01 | 未跟踪按钮号 200 | 视为保护丢失，不静默拥有 | `test scroll_input::tests::gate_treats_untracked_buttons_as_lost_protection … ok` | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-26 | 脚本启动独立 GPUI 精确像素夹具；开始截图冻结；`--headed` 拖拽建选区（前置）；AX 激活 `arcshot.overlay.scroll`；采集后 `mouse-click`/`mouse-wheel`/`press return --headed`；再开一轮 `press escape --headed` | 准备→输入安装→可信首帧→Collecting 全链路；像素真值逐格相等；状态+提示 AX 可寻址；单击不达目标、非自有键与选区内滚动达目标；Enter 复制 PNG 并清理 tap；Esc 放弃并返回原选区 | `mac_e2e_26=pass prepared=true collecting=true pixel_oracle=exact click_blocked=true scroll_passed=true enter=clipboard scrolls=1 clicks_blocked=1`；`mac_e2e_26_esc=pass abandon=true selection_returned=true`；Debug/Release 各 `cases=26` | Pass |

`agent-desktop status` 原始读数见环境段；被引用的日志行原文见上一节。
每步成功步的 `error.code` 为 `ok`（`agent()` 在 envelope `ok=false` 时直接失败）；
物理投递作为前置，`--headed` 坐标点击/拖拽本身不作为验收证据。

## Not tested

- **真实 TCC 切换、System Settings 返回重检、运行中撤权**：本机三项权限均已授权，
  改动 TCC 需用户参与（TEST.md §4.4）。三权限缺失通知的文案/按钮/重检策略由
  APP-EFFECT-38～41 与 `ScrollPermission::purpose()` 守护，实机切换保持
  **Not tested**。
- **TextEdit 固定文档与 Safari `file://` 静态页目标**：I-13 要求三类目标端到端，
  本票只在自有 GPUI 夹具（类 1）验证，另两类保持 **Not tested**。
- **真实物理双击完成与全局热键在采集中的重启**：注入式门缝（MAC-SCROLL-INPUT-32-03、
  APP-EFFECT-42/44、CORE-SESSION-56）覆盖决策逻辑；`agent-desktop` 本轮只投递了
  单击、滚轮与 Return/Escape，真实双击与 `⌘⇧X` 重启保持 **Not tested**。
- **物理输入设备的按下释放归属细节**：合成事件的 HID 源状态在本机对 Esc 有粘性，
  tap 的键状态预检因此只保留物理鼠标按钮；真实按键的 key-up 时序保持 **Not tested**。
  产品用「已消费意图键在 drain 时释放归属 + 2 秒有界强制清理」兜底，日志为
  `drain_forced=false`（本轮两次 E2E 均为 false）。
- **SCK 请求错误/超时的实机注入、worker panic 的实机复现、24M 像素/2 GiB 峰值**：
  只有注入式 APP-ASYNC-14/15 与 IMAGE-LIMIT/CORE-COORD 接缝覆盖；实机故障注入
  与性能/资源门槛保持 **Not tested**。
- **IME 候选窗、多显示器、视觉观感**（控制窗排版、状态文字审美）：属 §4.4 用户项，
  保持 **Not tested**。
- **#33 全部行为**：多帧拼接、唯一位移验证、反向去重、上下追加、实时已验证预览、
  三秒完成预览均未实现，保持 **Not tested**；本票没有以阶段性子流程冒充完整 V1。

## 给下一票（#33）的接缝

- 帧接纳走 `arcshot_app::admit_scroll_capture(region, crop, …)`，输出
  `CapturedFrame` 的几何由唯一的 `arcshot_core::scroll_frame_geometry` 换算；
  `ScrollPreparation::admit_frame` 当前刻意只保存 1 帧，第二帧被忽略而不排队。
  #33 应把「已验证帧存储」接在 `ScrollFrameAdmitted` 之后，并在此扩展为
  ≤2 原始帧队列 + 背压（本票未造队列，也未静默丢帧）。
- `ScrollPhase::Collecting` 由 `CaptureEffect::ScrollCollecting` 打开；
  `TapGate::open_collection` 只在固定选区内放行原始滚动。预览扩展不得新建
  UI 窗口：当前 `install_scroll_tap` 要求采集前唯一仍可见自有窗口恰好是控制窗
  （`scroll_exclusion_fixed prepared=2 collecting=1 control=…`），任何新窗口都会
  被拒启。
- 完成路径复用 `Session::begin_output`/`ComposeOutput`/`publish_output`；
  `complete_scroll` 把文档临时换成「整帧选区的完成文档」并把区域文档stash，
  `OutputCompleted` 成功结束会话、失败恢复区域文档。拼接结果要替换这个完成文档，
  但不要绕过版本栅栏与 PNGf/保存字节一致性。
- 失败分级：`CaptureFailure::{NativeUncertain, WorkerPanicked}` 与
  `ScrollInputError::ProtectionLost` 分别是终态/已知停止；`DisplayInvalid`
  取消整会话。不要把不确定请求当普通拼接失败重试。
- 原生侧 `capture_scroll_region` 与 `capture_builtin` 共用 `capture_display_region`、
  同一排除数组、命名 sRGB + SDR、BGRA→RGBA 接纳与不透明检查；换 `sourceRect`
  只需改 `CaptureRegion`，不要复制第二条 SCK 路线。
