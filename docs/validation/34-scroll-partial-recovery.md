# #34 滚动失败的显式部分结果与恢复验证记录

本记录覆盖 #34 落地的 I-11 步骤 11 与 I-12 失败分级：已知状态停止（内容变化、
重叠歧义、目标/权限/输入保护/资源）且有已验证拼接历史时，只提供「使用已验证
部分 / 放弃并返回原选区」两项显式选择，不自动输出不完整内容；无可用已验证结果
时只放弃；原生状态不确定（SCK 错误/超时、worker panic、不可确认请求）与内屏
失效从不提供部分、禁止会话内重启。证据来自 core 会话语义、app 纯选择分类、
自有 GPUI 精确像素夹具的受控 Gap 停止 E2E（Debug/Release），以及
Debug/Release 两套完整 `scripts/check-all.sh`。

三秒完成预览、长图标注与 TextEdit/Safari 目标全部**未实现、未声称通过**。

## 实测来源与环境

- 基线：`spec/v1` = `45819de`（Merge ticket/33-scroll-stitch）。
- 实测实现/脚本 SHA：`6feb8a56502acda182c735476b4ca7791d9eed76`
  （`feat(scroll): explicit partial-result choice after known-state stops`）。
  本证据文档是后续纯文档提交，不改产品源码、脚本或测试。
- 时间：2026-11-04T22:35:56Z（Debug/Release E2E 与 `scripts/check-all.sh`
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
- 显示器：内建屏，1470×956 逻辑点，scale=2；`system_profiler` 报面板
  2560×1664 Retina。产品日志滚动帧
  `scroll_frame_request_us=… pixels=460x360 stride=1840 scale=2`（选区
  230×180 逻辑点）；已验证部分为 `pixels=460x424`（原帧 460x360 + 两次
  32 物理行扩展）。未保存屏幕图像。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 为
  `accessibility=granted`、`screen_recording=granted`、`automation=unknown`、
  `platform=macos`；被测 bundle 的预检日志为
  `scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`。
  未改动 TCC，未请求通知权限，未注册/注销登录项。
- 夹具：`fixture=scroll-single-frame-e2e-v1`（自有 GPUI 精确像素夹具，独立进程、
  无标题栏、origin=(600,120) size=760x720、initial_offset=2000、MAX_OFFSET=7400；
  本票新增按键 `j` 原子跳变 +1000 行，`fixture_jump offset_points=…` 回执）。
- 日志：`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-release.log`、
  `target/e2e-scroll-fixture-debug.log`、
  `target/e2e-scroll-fixture-release.log`、`/tmp/34-check-all.log`。日志是本地
  构建产物，不入库；本报告只保存文本/数值/hash 摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本证据文档是
  后续纯文档提交。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test scroll_loop
cargo test --locked -p arcshot-app --test scroll_partial_flow
scripts/check-all.sh
scripts/e2e-agent-desktop.py              # Debug
scripts/e2e-agent-desktop.py --release    # Release
```

所有原生门禁运行都在同一个 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行
执行，并在每次运行前确认 `agent-desktop list-apps --app "ArcShot"` 为空。

- `cargo clippy … -D warnings`：exit 0，无 error。
- `cargo test --locked --workspace --all-features`：exit 0，全部测试二进制
  `test result: ok`。
- `cargo test --locked -p arcshot-core --test scroll_loop`：exit 0，33 passed。
- `cargo test --locked -p arcshot-app --test scroll_partial_flow`：exit 0，
  7 passed。
- `scripts/check-all.sh`：exit 0；关键行：

```text
validation_tools=pass
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
privacy_smoke=pass resident_pid=50699 network_sockets=0 lsof_status=1
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示
  （非 Clippy `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=28`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release log=target/e2e-agent-desktop-release.log cases=28`。

### MAC-E2E-28 的原生诊断行（Debug）

```text
scroll_stitch extended offset=32 added_rows=32 top=0 bottom=392 extensions=2 duplicates=…
fixture_jump offset_points=3032
scroll_stitch_refused reason=scroll displacement leaves an unverifiable gap
scroll_phase from=Collecting to=AwaitingPartialChoice action=ScrollStitchRefused
scroll_partial_choice available=true reason=Stitch Gap pixels=460x424
scroll_input_retired ticket=CaptureTicket(10) scrolls=2 clicks_blocked=0 keys_owned=0 drain_forced=false
scroll_partial_choice action=abandon ticket=CaptureTicket(10)

scroll_stitch_refused reason=scroll displacement leaves an unverifiable gap
scroll_phase from=Collecting to=AwaitingPartialChoice action=ScrollStitchRefused
scroll_partial_choice available=true reason=Stitch Gap pixels=460x424
scroll_input_retired ticket=CaptureTicket(11) scrolls=2 clicks_blocked=0 keys_owned=0 drain_forced=false
scroll_partial_choice action=accept ticket=CaptureTicket(11)
output_publish target=Clipboard bytes=42547
```

- 放弃子用例：Gap 停止后控制窗出现 `arcshot.scroll.partial.use` 与
  `arcshot.scroll.partial.abandon` 两个 `button`，`arcshot.scroll.hint` 含
  “已验证部分不完整；确认后才会复制”；点击前日志无 `output_publish`；
  点击放弃后出现 `action=AbandonScrollPartial`，冻结覆盖层与原选区返回
  （`arcshot.overlay.save` 重新可寻址），整段无输出。
- 接受子用例：同样的 Gap 停止后，点击前无 `output_publish`；点击
  `arcshot.scroll.partial.use`（`a11y_action=arcshot.scroll.partial.use action=Click`）
  后才出现 `output_publish target=Clipboard bytes=42547` 与
  `from=Frozen to=Resident action=OutputCompleted`。接受路径复用正常完成的
  `ComposeOutput`/`publish_output` 栅栏，没有第二条输出管线；本票没有
  三秒完成预览可误显示，失败路径由 CORE-SESSION-72 / APP-EFFECT-49 证明
  不出现成功。
- 夹具回执（Debug `target/e2e-scroll-fixture-debug.log`）：

```text
fixture_scroll delta_points=-16 offset_points=2016
fixture_scroll delta_points=-16 offset_points=2032
fixture_jump offset_points=3032
fixture_scroll delta_points=-16 offset_points=3048
fixture_scroll delta_points=-16 offset_points=3064
fixture_jump offset_points=4064
```

  大滚轮会被 GPUI 拆成多步事件并可能逐步扩展，因此缺口用原子跳变制造；
  这与“快跳本身不武断拒绝、无法验证则停”的规格一致：跳变后若仍有重叠
  且唯一可验证，拼接本可继续，本用例的跳变越过全部已验证行。

### MAC-E2E-24 的同步变化（Debug）

MAC-E2E-24 的选区落在 Rex 终端上，采集开始后动态内容造成
`scroll_stitch_refused reason=scroll content changed inside the overlap`。
本票起该已知停止提供部分选择（`scroll_partial_choice available=true
reason=Stitch ContentChanged pixels=600x400`），脚本点击
`arcshot.scroll.partial.abandon` 后区域会话与选区返回，`mac_e2e_24=pass`。

## 稳定自动化子用例（Rust）

前置均为上述 SHA、锁定依赖与注入式夹具；「实际」为对应 `cargo test` 的观察行。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-SESSION-67 | Collecting；验证扩展为 460x424 文档后注入拼接拒绝 Gap | 提供部分选择、不自动输出；接受后以扩展文档走共享 Clipboard Finish，成功结束会话 | `test core_session_67_stitch_refusal_with_verified_history_offers_partial_not_output … ok` | Pass |
| CORE-SESSION-68 | Collecting；分别注入内容变化/歧义/几何拼接拒绝、目标丢失、输入保护丢失、帧准入/资源失败 | 每种已知停止都进入 `AwaitingPartialChoice` 并提供部分，无输出 | `test core_session_68_every_known_collecting_stop_offers_the_partial_choice … ok` | Pass |
| CORE-SESSION-69 | Collecting；拼接资源超限 | 提供部分，接受仍走预算栅栏的共享输出 | `test core_session_69_resource_budget_stop_during_collecting_offers_the_partial … ok` | Pass |
| CORE-SESSION-70 | 采集前：权限缺失；首帧准入/几何/资源/worker 失败 | 只放弃：`ScrollRefused` + `PresentFrozen`，部分不可用，接受动作被忽略 | `test core_session_70_stops_before_collection_offer_only_abandon … ok` | Pass |
| CORE-SESSION-71 | 等待选择；放弃 | 原选区与版本返回，`PresentFrozen`；迟到接受/完成/拼接/帧全部忽略；可重新进入滚动 | `test core_session_71_abandon_returns_the_pre_scroll_selection_and_clears_the_partial … ok` | Pass |
| CORE-SESSION-72 | 等待选择；接受后分别注入 Clipboard/Encode/Worker/Resource 输出失败 | 无成功（不 Resident、无音效），回到选择供重试/放弃；重试成功，失败后放弃也回原选区 | `test core_session_72_failed_partial_output_keeps_the_result_for_retry_or_abandon … ok` | Pass |
| CORE-SESSION-73 | Collecting 中显式取消；等待时 Esc 取消 | 从不提供部分，直接 `PresentFrozen` | `test core_session_73_explicit_cancel_during_collecting_never_offers_a_partial … ok` | Pass |
| CORE-SESSION-74 | Collecting 期与等待期分别注入 SCK 不确定/worker panic/内屏失效 | 终态与整会话取消，从不提供部分；`Start` 被原生退出栅栏拒绝 | `test core_session_74_uncertain_native_state_and_display_loss_never_offer_a_partial … ok` | Pass |
| CORE-SESSION-75 | 等待时热键重启；等待时重入滚动 | 热键重启过原生退出栅栏并清除部分，迟到接受忽略；重入被拒且不覆盖部分 | `test core_session_75_hotkey_restart_and_reentry_cannot_steal_or_pollute_the_choice … ok` | Pass |
| CORE-SESSION-76 | 部分接受成功后投迟到接受/放弃/完成/重复输出结果 | 全部忽略，Resident 不重复输出 | `test core_session_76_late_results_after_partial_success_never_output_again … ok` | Pass |
| CORE-SESSION-57/58/64/65（同步） | 采集期输入保护丢失/目标丢失/拼接拒绝/采集帧失败 | 期望改为部分选择（与 CORE-SESSION-68 同义），其余断言不变 | `test core_session_57/58/64/65 … ok` | Pass |
| APP-EFFECT-45 | 已知失败族（内容变化/歧义/缺口/几何/资源/帧/目标/输入/窗口/权限）× 有已验证结果 | 恰好两项选择：使用已验证部分、放弃 | `test app_effect_45_known_failures_with_verified_result_offer_exactly_two_choices … ok` | Pass |
| APP-EFFECT-46 | 同一失败族 × 无已验证结果；目标/窗口不确定状态 × 有结果 | 只提供放弃 | `test app_effect_46_no_verified_result_or_uncertain_state_offers_only_abandon … ok` | Pass |
| APP-EFFECT-47 | 读取共享标签常量 | 使用已验证部分 / 放弃并返回原选区；警告含「不完整」与「确认」 | `test app_effect_47_choice_labels_warn_that_the_partial_is_incomplete … ok` | Pass |
| APP-EFFECT-48 | 两种选择 × 当前 ticket | 映射到 `AcceptScrollPartial` / `AbandonScrollPartial` 显式动作 | `test app_effect_48_choices_map_to_the_explicit_session_actions_with_the_live_ticket … ok` | Pass |
| APP-EFFECT-49 | 真实会话走到选择，接受后注入 Clipboard 失败 | 回到选择（无成功音效、Frozen、部分仍可用），仍提供两项 | `test app_effect_49_failed_partial_output_still_offers_retry_or_abandon … ok` | Pass |
| APP-ASYNC-21 | 等待选择时投 tap 的 Finish/Cancel（含放弃后迟到） | 均不路由：不能自动输出，也不能经 tap 重启 | `test app_async_21_tap_commands_while_awaiting_the_choice_route_nowhere … ok` | Pass |
| APP-ASYNC-22 | 无准备时投不确定状态；准备期原生捕获不确定 | 无准备不重启；准备期不确定终态（`RelaunchRequired`） | `test app_async_22_uncertain_state_while_awaiting_discards_the_partial … ok` | Pass |

受控负例与注入族的对应关系（负面通过只证该例被安全处理，不构成
兼容或“识别所有动态内容”的保证）：动态内容/固定表头或浮层 → 重叠不一致
（`ContentChanged`，CORE-SESSION-68）；重复/低纹理 → 歧义（`Ambiguous`，
IMAGE-STITCH-65 在 #33 已有）；无重叠快跳 → 缺口（`Gap`，MAC-E2E-28 原子
跳变实机）；横向/对角/嵌套滚动 → 无唯一垂直位移解释该帧（`Gap`/
`ContentChanged` 分类，CORE-SESSION-68 同族）；目标/权限/输入保护失效 →
采集前只放弃（CORE-SESSION-70）或采集期提供部分（CORE-SESSION-68），
权限缺失通知分支本身保持 Not tested（见下）。

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-24（同步） | Rex 终端动态内容采集期停止 | 部分选择出现，点击放弃后覆盖层与选区返回 | `mac_e2e_24=pass entry_gate=ax selection_precondition=headed_drag missing=none` | Pass |
| MAC-E2E-26（回归） | 自有夹具单帧会话 | 像素真值、单击拦截、滚动放行、Enter 完成、Esc 返回原选区 | `mac_e2e_26=pass … pixel_oracle=exact click_blocked=true scroll_passed=true enter=clipboard`；`mac_e2e_26_esc=pass` | Pass |
| MAC-E2E-27（回归） | 右/左/内侧三位置双向拼接 | 扩展/去重/像素真值/输出 Clipboard | `mac_e2e_27_right/left/inner=pass` | Pass |
| MAC-E2E-28 放弃 | 慢速扩展后原子跳变 Gap 停止 | 两项 AX 选择与不完整警告；点击前无输出；放弃回原选区且无输出 | `mac_e2e_28_abandon=pass gap_stop=true choices=two incomplete_warn=true abandon=selection_returned output=none` | Pass |
| MAC-E2E-28 接受 | 同上 | 点击前无输出；点击接受后才 Clipboard 输出并结束会话 | `mac_e2e_28_accept=pass gap_stop=true choices=two no_auto_output=true accept=clipboard after_explicit_click` | Pass |

Release E2E 的 `mac_e2e_28_abandon` / `mac_e2e_28_accept` 同样通过
（`cases=28`）。

## Not tested

- **真实 TCC 切换、System Settings 返回重检、运行中撤权**：本机三项权限均已
  授权，改动 TCC 需用户参与（TEST.md §4.4）。权限失效在 core/app 以注入
  证明只放弃（CORE-SESSION-70、APP-EFFECT-45/46），原生撤权时序保持
  **Not tested**；若系统设置切换先触发焦点停止，只记录实际顺序，本票
  未冒称直接撤权路径。
- **原生 SCK 错误/超时的实机注入、worker panic 实机复现**：只有注入式
  APP-ASYNC-14/15/22 与 CORE-SESSION-74 覆盖“禁止部分与会话内重启”；
  实机故障注入保持 **Not tested**。
- **TextEdit 固定文档与 Safari `file://` 静态页目标**：只在自有 GPUI 夹具
  （类 1）验证，另两类保持 **Not tested**。
- **动态/固定表头/重复低纹理/横向·对角·嵌套滚动的实机通过**：本票以注入
  族证明安全停止与选择，MAC-E2E-24 的 Rex 动态内容是偶然观察（记录
  `ContentChanged`），不是对这些类别的兼容承诺；其余实机负例保持
  **Not tested**，不得外推。
- **接受部分后编码/复制失败的实机注入**：由 CORE-SESSION-72 与
  APP-EFFECT-49 的注入结果覆盖（保留重试/放弃、无成功预览）；真实剪贴板
  拒绝的原生注入保持 **Not tested**。
- **三秒完成预览与长图标注**：I-11 步骤 7/8 不在本票范围，未实现。
- **IME 候选窗、多显示器、视觉观感**（控制窗排版、部分缩略图审美）：
  属 §4.4 用户项，保持 **Not tested**。
- **24M 像素 / 2 GiB 资源峰值与长会话性能**：保持 **Not tested**。

## 给下一票的接缝

- 部分状态在 core：`ScrollPhase::AwaitingPartialChoice` + `ScrollPreparation::stitch_frame`
  保留已验证文档，`CaptureSession::scroll_partial_available()` /
  `scroll_stitch_frame()` 暴露它；`AcceptScrollPartial` 与正常完成共用
  `begin_scroll_document_output` → `begin_output`/`ComposeOutput`/`publish_output`，
  输出失败通过 `partial_output` 标记回到选择（`ScrollPartialChoice` +
  `ReportOutputFailure`，顺序如此以便失败文案最后落到控制窗）。
- 选择分类在 app 纯函数 `arcshot_app::scroll_partial_flow`：
  `partial_offered` / `partial_choices` / `partial_action` 与标签常量
  `PARTIAL_ACCEPT_LABEL` / `PARTIAL_ABANDON_LABEL` / `PARTIAL_INCOMPLETE_WARNING`；
  控制窗按钮 `arcshot.scroll.partial.use` / `.abandon` 直接引用它们。
- 控制窗在选择时保留、输入保护退役、已验证预览保留、未验证队列清空；
  诊断为 `scroll_partial_choice available=true|false … pixels=…`、
  `scroll_partial_choice action=accept|abandon …`、`scroll_input_retired …`。
- 三秒完成预览票需要成功预览时，应挂在成功输出之后，而不是本票的部分
  失败路径；当前成功路径仍直接 `DismissOverlay` + `Resident`。
