# #31 Scroll Capture 入口、三权限与目标准备验证记录

本记录覆盖 #31 落地的 I-11 步骤 1–2 与 I-12 的权限/目标半边：从当前 Capture
Session 的有效选区进入滚动准备、三权限（屏幕录制 / 输入监控 / 辅助功能）预检、
目标窗口公开身份锁定、ArcShot 自有可见窗口集固定，以及已知状态失败与原生不确定
终态的分级。证据来自 core 语义动作、app 注入式权限策略与有界目标探测、原生三权限
预检/公开 hit-test/自有窗口映射、真实 AX 驱动的 E2E 与完整 `scripts/check-all.sh`。
滚动采样循环、拼接与预览窗口由 #32/#33 拥有，本票只到“准备完成”接缝，不宣称
“可以滚动”。真实 TCC 切换、目标应用前台/指针 hit-test 的实机时序与缺失权限通知
分支保持 **Not tested**。本票没有保存用户截图、没有联网、没有改动 TCC。

## 实测来源与环境

- 基线：`spec/v1` = `ed74641`（含 #22/#23/#24/#25/#26/#27/#29/#30）；#31 功能提交
  `1b89bd6`。
- 实测实现/脚本 SHA：`2ba36bc4953d74950eddc1c0c16b4a22ad1efb9f`（#31 功能 `1b89bd6`、设置返回重检修复 `2ba36bc` + 测试 +
  `TEST.md`/`scripts/e2e-agent-desktop.py`）；本证据文档是后续纯文档提交，不改代码或脚本。
- 时间：2026-11-04T10:40:33Z（`scripts/check-all.sh` 完成）；Debug E2E 与 Release E2E
  分别在 2026-11-04 完成。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；均由 `scripts/build-app.sh` 重建，本机
  自签名身份 `ArcShot Dev`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)，
  `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk`。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，Target arm64-apple-macosx27.2.0。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏 ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；产品日志
  `capture_request_us=… display=1 pixels=2940x1912 stride=11760 scale=2` 给出物理
  2940×1912；`system_profiler` 报面板 2560×1664 Retina。未保存屏幕图像。
- 权限：`agent-desktop status` 为 `accessibility=granted`、`screen_recording=granted`、
  `automation=unknown`、`platform=macos`；被测 bundle 的三权限预检在目标识别前日志
  为 `scroll_permission screen_recording=true input_monitoring=true accessibility=true
  missing=none`。未改动 TCC，未请求通知权限，未注册/注销登录项。
- 夹具：`fixture=scroll-entry-e2e-v1`：真实 Debug/Release bundle、core/app 注入式
  权限与目标探测夹具、`agent-desktop 1.1.0`（envelope `2.6`）、真实 `NSUserDefaults`
  偏好域与真实 `SMAppService` 只读状态。无图像夹具、无外部应用夹具。
- 日志：`target/e2e-agent-desktop-debug.log`、`target/e2e-agent-desktop-release.log`、
  `/tmp/31-check-all.log`。日志是本地构建产物，不入库；本报告只保存文本/数值摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本报告是后续纯证据
  文档提交，不改代码或脚本。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test scroll
cargo test --locked -p arcshot-app --test scroll_flow
cargo test --locked -p arcshot-app --test scroll_target_tasks
scripts/check-all.sh > /tmp/31-check-all.log 2>&1
scripts/e2e-agent-desktop.py > /tmp/31-e2e-debug.log 2>&1
scripts/e2e-agent-desktop.py --release > /tmp/31-e2e-release.log 2>&1
```

- `cargo clippy … -D warnings`：exit 0，`Finished dev profile`。
- `cargo test --locked --workspace --all-features`：exit 0，全部测试二进制
  `test result: ok`，共 204 个 Rust 测试通过。
- `scripts/check-all.sh`：exit 0；关键行（Debug 与 Release 各一份原生冒烟）：

```text
validation_tools=pass
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
capture_menu_smoke=pass queued_commands=6 menu_items=7 no_native_capture=true
privacy_smoke=pass resident_pid=56244 network_sockets=0 lsof_status=1
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
capture_menu_smoke=pass queued_commands=6 menu_items=7 no_native_capture=true
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示（非 Clippy
  `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug … cases=24`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release … cases=24`。

## 稳定自动化子用例（Rust）

前置为上述 SHA、锁定依赖与注入式夹具；「实际」列为 `cargo test --locked --workspace
--all-features` 的观察行。所有结果均为 **Pass**。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-SESSION-28 | 冻结会话无选区时 `EnterScroll` | 拒绝 `NoSelection`，区域会话保持 Frozen | `test core_session_28_enter_scroll_requires_a_valid_selection … ok` | Pass |
| CORE-SESSION-29 | 冻结会话有选区与一个标注时 `EnterScroll` | 拒绝 `AnnotationsPresent`，标注与版本不变 | `test core_session_29_enter_scroll_refuses_annotations_without_dropping_them … ok` | Pass |
| CORE-SESSION-30 | 有效选区 `EnterScroll` | `DismissOverlay`+`CheckScrollPermissions`，沿用同一选区，不另起会话 | `test core_session_30_enter_scroll_reuses_the_selection_and_checks_three_permissions … ok` | Pass |
| CORE-SESSION-31 | 三权限缺失（输入监控/辅助功能） | 停止并保留区域会话与选区，无输出 | `test core_session_31_missing_scroll_permissions_stop_and_keep_the_region_session … ok` | Pass |
| CORE-SESSION-32 | 三权限均可用 | 进入目标识别 | `test core_session_32_all_permissions_proceed_to_target_identification … ok` | Pass |
| CORE-SESSION-33 | 目标窗口不包含选区 / 完整包含选区 | 前者 `Target(Geometry)` 拒绝；后者锁定并进入排除集准备 | `test core_session_33_target_lock_requires_the_selection_inside_the_window … ok` | Pass |
| CORE-SESSION-34 | 目标 `NoTarget`/`AmbiguousTarget`/`Offscreen`/`NotFrontmost`/`PointerMismatch` | 各自已知失败停止并保留区域会话 | `test core_session_34_known_target_failures_stop_and_keep_the_region_session … ok` | Pass |
| CORE-SESSION-35 | 自有窗口映射失败 | `Windows(MappingFailed)` 拒绝，无输出 | `test core_session_35_window_exclusion_mapping_failure_refuses_without_output … ok` | Pass |
| CORE-SESSION-36 | 目标锁定 + 排除集映射成功 | `ScrollPrepared`，排除集固定 | `test core_session_36_successful_preparation_fixes_the_exclusion_set … ok` | Pass |
| CORE-SESSION-37 | 准备中 `CancelScroll` | 保留选区，区域会话保持 Frozen | `test core_session_37_cancel_scroll_keeps_the_region_selection … ok` | Pass |
| CORE-SESSION-38 | 准备中 `ScrollNativeUncertain` | 终态 RelaunchRequired，`Start` 只报告不确定，禁止会话内重启 | `test core_session_38_uncertain_native_state_is_terminal_and_forbids_restart … ok` | Pass |
| CORE-SESSION-39 | 准备中 `DisplayInvalidated` | 取消整个会话，不迁移、不输出部分 | `test core_session_39_display_loss_cancels_the_whole_session … ok` | Pass |
| CORE-SESSION-40 | 准备中热键 `Start` | 放弃准备并进入新捕获预检 | `test core_session_40_hotkey_restart_abandons_scroll_preparation … ok` | Pass |
| CORE-SESSION-41 | 取消后迟到的 `ScrollWindowsPrepared` | 不影响新尝试 | `test core_session_41_stale_scroll_actions_never_affect_another_attempt … ok` | Pass |
| CORE-SESSION-42 | 零尺寸目标窗口 | `Target(Geometry)` 拒绝 | `test core_session_42_selection_outside_a_zero_extent_window_is_geometry … ok` | Pass |
| CORE-SESSION-43 | 三权限键序、标签、用途 | 键序固定、用途非空且不归因 TCC | `test core_session_43_scroll_permission_matrix_is_specific_and_content_free … ok` | Pass |
| APP-EFFECT-38 | 缺失权限 → 设置 → 返回 → 重检 → 取消 | 通知/设置/重检/取消顺序正确，返回只重检不启动 | `test app_effect_38_missing_permissions_offer_settings_recheck_and_cancel … ok` | Pass |
| APP-EFFECT-39 | 缺失权限后 `Recheck` → 三权限可用 | 只显示 Ready，不自动启动 | `test app_effect_39_recheck_never_auto_starts_scroll … ok` | Pass |
| APP-EFFECT-40 | 缺失权限并打开设置后 `Cancel` | 通知与设置等待清空，迟到的返回无效 | `test app_effect_40_cancel_clears_the_notice_and_settings_wait … ok` | Pass |
| APP-EFFECT-41 | 初始三权限可用 | 只产生 `ShowNotice(Ready)`，无预检/设置副作用 | `test app_effect_41_ready_notice_does_not_emit_a_start_effect … ok` | Pass |
| APP-ASYNC-10 | 探测 worker 阻塞至绝对期限 | `NativeUncertain` 终态，禁止再次提交 | `test app_async_10_absolute_deadline_is_independent_of_sck_environment … ok` | Pass |
| APP-ASYNC-11 | 已知目标失败 | 非终态，闸门可重开 | `test app_async_11_known_target_failure_is_not_terminal … ok` | Pass |
| APP-ASYNC-12 | 单在途探测 | 第二次提交 Busy；已知结果后重开 | `test app_async_12_single_inflight_gate_reopens_only_after_a_known_result … ok` | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-24 | 开始截图冻结；对 `arcshot.overlay.scroll` 发 AX 点击（无选区）；`drag --from-xy 400,300 --to-xy 700,500 --headed` 创建选区；再 AX 激活入口 | 入口始终以 `button` 可寻址；无选区点击 `POLICY_DENIED` 且无 `a11y_action`；拖拽后 `capture_edit action=SelectRegion … selection=Some([400.0, 267.0, 300.0, 200.0])`；入口点击出现 `a11y_action=arcshot.overlay.scroll action=Click`、`scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`、`scroll_target_failure reason=AmbiguousTarget`、`scroll_refused reason=Target AmbiguousTarget`；区域覆盖层保留 | `mac_e2e_24=pass entry_gate=ax selection_precondition=headed_drag missing=none` | Pass |

  Debug 与 Release 两份日志的对应行：

```text
level=INFO target=arcshot::coordinator capture_edit action=SelectRegion version=1 annotations=0 selection=Some([400.0, 267.0, 300.0, 200.0]) tool=Select selected=None style=none
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.scroll action=Click
level=INFO target=arcshot::coordinator scroll_phase from=Inactive to=CheckingPermissions action=EnterScroll
level=INFO target=arcshot::coordinator scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none
level=INFO target=arcshot::coordinator scroll_phase from=CheckingPermissions to=IdentifyingTarget action=ScrollPermissionsChecked
level=INFO target=arcshot::coordinator scroll_target_failure reason=AmbiguousTarget
level=INFO target=arcshot::coordinator scroll_phase from=IdentifyingTarget to=Inactive action=ScrollTargetIdentified
level=INFO target=arcshot::coordinator scroll_refused reason=Target AmbiguousTarget
```

  `AmbiguousTarget` 是本机环境结果：选区 (400,267)-(700,467) 同时被多个 layer-0
  窗口包含。这证明目标识别确实要求唯一包含选区的目标窗口，并走已知失败分支；它不
  证明 TextEdit/Safari 等单一目标窗口的锁定成功。

## Not tested

- **MAC-PERMISSION 三权限缺失通知分支**：本机三项权限均已授权，E2E 无法在不改动
  TCC 的前提下触发缺失通知。通知的用途文案、设置按钮与重检/取消策略由
  APP-EFFECT-38～41 与 `ScrollPermission::purpose()` 守护；真实 TCC 切换、系统设置
  返回时序与运行中失效属 §4.4，保持 **Not tested**。
- **目标锁定成功与控制窗**：本机目标识别返回 `AmbiguousTarget`，未到达
  `ScrollPrepared`。`scroll_exclusion_set()` 的真实 SCWindow 映射、控制窗
  `arcshot.scroll.status` / `arcshot.scroll.control.cancel` 的 AX 驱动与
  `scroll_prepared` 诊断保持 **Not tested**；核心侧由 CORE-SESSION-36 覆盖。
- **目标应用前台/指针 hit-test 的实机时序**：本机未构造“目标应用在前台且指针落在
  目标内”的受控场景；`probe_scroll_target` 的前台/hit-test 交叉核对逻辑已落地，但
  其实机正确性保持 **Not tested**。
- **滚动采样、拼接、预览与输入握手**：由 #32/#33 拥有，本票只到“准备完成”接缝。
- **`--headed` 拖拽只是选区前置**：MAC-E2E-24 的选区由物理拖拽创建，不作为指针
  手势的验收证据；入口的验收证据是 AX 状态与产品文本诊断。
