# #35 完成预览、长图标注与整图输出验证记录

本记录覆盖 #35 落地的 I-11 步骤 7–9：只有真实复制成功才保留整帧共享文档并显示
三秒等比缩略预览；激活预览进入同一文档的可滚动/可缩放标注面，用既有 Rectangle
编辑器标注；重新完成复制整张带标注长图，取消标注保留先前已复制的未标注版本。
证据来自 core 的成功后处理状态机、image 的整图带标注合成、app 的缩略图布局与
辅助功能创建路径、注入式调度夹具、自有 GPUI 精确像素夹具的真实 AX/文本诊断
E2E，以及 Debug/Release 两套完整 `scripts/check-all.sh`。

本票与 spec/v1 中已合入的 #34（显式部分结果选择）集成：普通滚动完成与「使用
已验证部分」现在共享同一套确认复制后的预览/标注/整图输出后处理。

三秒停留的动画观感、悬停不延长的主观判断、TextEdit/Safari 第三方目标保持
**Not tested**。

## 实测来源与环境

- 基线：合并前 `spec/v1` = `28d9903`（Merge ticket/34-scroll-recovery）；
  本票从 `45819de` 分出后将该 tip 合并入分支，冲突按双方意图解决。
- 实测实现/脚本 SHA：`2ff9b864bfbad7460e60155bc4f660dd772aedee`
  （`merge: align #34 partial-accept with the #35 completion preview`）。
  产品功能提交为 `4484a972cb706ce6dcbf877e774084598afc0735`；本证据文档是
  后续纯文档提交 `9cadf26f04b6b20d7376bbf3ec5d0b9435f4c1b0`。
- 时间：2026-11-05T00:35:00Z–2026-11-05T00:48:00Z（合并、Debug E2E 与
  Release E2E 依次完成）。
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
  2560×1664 Retina。产品日志完成帧 `pixels=460x360 stride=1840 scale=2`
  （230×180 逻辑点选区）；MAC-E2E-27 的实时预览/接纳帧为
  `pixels=1160x360 stride=4640 scale=2`（580×180 逻辑点选区）。未保存屏幕图像。
- 权限：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 为
  `accessibility=granted`、`screen_recording=granted`、`automation=granted`、
  `platform=macos`；被测 bundle 预检日志为
  `scroll_permission screen_recording=true input_monitoring=true accessibility=true missing=none`。
  未改动 TCC，未请求通知权限，未注册/注销登录项。
- 夹具：`fixture=scroll-single-frame-e2e-v1`（#32 的自有 GPUI 精确像素夹具，
  独立进程、无标题栏、origin=(600,120) size=760x720、initial_offset=2000、
  MAX_OFFSET=7400；#34 增加按键 `j` 的原子跳变 +1000 行）。像素真值由
  `fixture_pixel` 确定性函数计算。
- 日志：`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-release.log`、`/tmp/35-check-all-merged.log`、
  `/tmp/35-e2e-debug-final.log`、`/tmp/35-e2e-release-final.log`。日志是本地
  构建产物，不入库；本报告只保存文本/数值/hash 摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述实现 SHA 完全一致。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test scroll_loop
cargo test --locked -p arcshot-image --test annotations
cargo test --locked -p arcshot-app --test completion_preview --test key_policy
scripts/check-all.sh                      # Debug + Release startup smoke
scripts/e2e-agent-desktop.py              # Debug
scripts/e2e-agent-desktop.py --release    # Release
```

所有原生门禁运行都在同一个 `mkdir /tmp/arcshot-native-gate.lock` 目录锁下串行
执行，并在每次运行前确认 `agent-desktop list-apps --app "ArcShot"` 为空。

- `cargo clippy … -D warnings`：exit 0，无 error。
- `cargo test --locked --workspace --all-features`：exit 0，47 个测试二进制
  全部 `test result: ok`。
- `cargo test --locked -p arcshot-core --test scroll_loop`：exit 0，
  39 passed。
- `cargo test --locked -p arcshot-image --test annotations`：exit 0，
  8 passed。
- `cargo test --locked -p arcshot-app --test completion_preview --test key_policy`：
  exit 0，3 passed + 18 passed。
- `scripts/check-all.sh`：exit 0；关键行：

```text
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
privacy_smoke=pass resident_pid=… network_sockets=0 lsof_status=1
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
clipboard_pngf_smoke=pass width=2 height=2 private_pasteboard=true
save_file_smoke=pass bytes=87 roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示
  （非 Clippy `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=28`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release log=target/e2e-agent-desktop-release.log cases=28`。

### MAC-E2E-29 的原生诊断行（Debug，Release 同形且 hash/字节一致）

```text
level=INFO target=arcshot::coordinator output_publish target=Clipboard bytes=35957 pixels=460x360 annotations=0
level=INFO target=arcshot::coordinator completion_preview shown ticket=CaptureTicket(5) frame=[1045,620,409,320] pixels=460x360
level=INFO target=arcshot::ui a11y_action=arcshot.completion.preview action=Click
level=INFO target=arcshot::coordinator completion_annotation_opened ticket=CaptureTicket(5) document=230x180 zoom=1.000
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.completion.add_rectangle action=Click
level=INFO target=arcshot::coordinator capture_edit action=AddDefaultRectangle version=2 annotations=1 selection=Some([0.0, 0.0, 230.0, 180.0]) tool=Select selected=None style=none
level=INFO target=arcshot::coordinator output_publish target=Clipboard bytes=36438 pixels=460x360 annotations=1
level=INFO target=arcshot::coordinator capture_phase from=Frozen to=Resident action=OutputCompleted
level=INFO target=arcshot::coordinator output_publish target=Clipboard bytes=39955 pixels=480x320 annotations=0
level=INFO target=arcshot::coordinator completion_preview shown ticket=CaptureTicket(6) frame=[1034,660,420,280] pixels=480x320
level=INFO target=arcshot::ui a11y_action=arcshot.completion.preview action=Click
level=INFO target=arcshot::coordinator completion_annotation_opened ticket=CaptureTicket(6) document=240x160 zoom=1.000
level=INFO target=arcshot::coordinator capture_phase from=Frozen to=Resident action=CancelAttempt
```

- 第一个剪贴板 PNG：460×360、35,957 字节、SHA-256 前缀 `122ab4a09eff`。
  重新完成后：460×360、36,438 字节、SHA-256 前缀 `46c1dede1b7f`。尺寸等于
  230×180 逻辑点选区的完整物理 crop，字节/hash 变化证明整图带标注被复制。
- 取消轮：第二次完成后的剪贴板 PNG 为 480×320、39,955 字节、
  SHA-256 前缀 `aec8c441f5b7`；点击预览进入标注后 Esc 取消，剪贴板 SHA-256
  与前一次读取完全一致，且 `output_publish` 计数不再增加，证明取消保留了
  先前未标注版本。

### 稳定自动化子用例（Rust）

前置均为上述 SHA、锁定依赖与注入式夹具；「实际」为对应 `cargo test` 的观察行。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-SESSION-52（本票改写） | Collecting；完成复制成功 | 请求 `ShowCompletionPreview`、保持整帧共享文档与音效偏好 | `test core_session_52_successful_completion_shows_the_preview_and_keeps_the_shared_document … ok` | Pass |
| CORE-SESSION-53（本票改写） | Collecting；复制失败 | 不产生预览标记、保留采集并可重试 | `test core_session_53_failed_completion_copy_shows_no_preview_and_allows_retry … ok` | Pass |
| CORE-SESSION-77 | 预览成功后取消 | 会话结束且不产生第二个输出 | `test core_session_77_cancel_after_the_completion_preview_ends_without_output … ok` | Pass |
| CORE-SESSION-78 | 点击进入共享文档→绘制 Rectangle→重新完成 | 整帧 crop+标注入 job，成功后结束会话 | `test core_session_78_re_finish_outputs_the_whole_annotated_image_and_then_ends … ok` | Pass |
| CORE-SESSION-79 | 普通区域 Finish | 直接结束、无预览标记 | `test core_session_79_ordinary_region_finish_still_ends_without_a_preview … ok` | Pass |
| CORE-SESSION-80 | 点击进入后无编辑直接 Finish | 整帧复制并结束，不二次预览 | `test core_session_80_re_finish_without_edits_still_ends_the_session … ok` | Pass |
| CORE-SESSION-81 | 预览显示期间直接 Finish | 视为重新完成并结束，不重启预览 | `test core_session_81_finish_during_the_preview_is_a_re_finish_not_a_second_preview … ok` | Pass |
| CORE-SESSION-82 | 辅助功能矩形路径；输出期复测 | 同一 `add_rectangle` 生成内缩矩形；输出期被栅栏 | `test core_session_82_accessibility_rectangle_enters_the_shared_document … ok` | Pass |
| CORE-SESSION-67～76（#34 集成同步） | 接受已验证部分后复制成功 | 与本票共享预览后处理，会话保持 Frozen 等待标注/取消 | `core_session_67/72/76 … ok`（断言已更新为 `ShowCompletionPreview` + `Frozen`） | Pass |
| IMAGE-COMPOSE-35 | 230×80 物理高长文档+底部 Rectangle，完整 crop 合成 | 输出整幅 20×80，底部描边为红、下方与末行仍输出 | `test image_compose_35_a_long_document_outputs_its_whole_extent_with_annotations … ok` | Pass |
| APP-EFFECT-50 | 580×3700 逻辑长图、1470×956 视口 | 高等比缩到 320 高、宽 50、右下定位 | `test app_effect_50_a_tall_long_image_fits_the_height_cap_proportionally … ok` | Pass |
| APP-EFFECT-51 | 2940×1912 逻辑宽图 | 宽限 420、高随比例、四边留 margin | `test app_effect_51_a_wide_image_fits_the_width_cap_and_stays_in_the_margin … ok` | Pass |
| APP-EFFECT-52 | 1000×800 逻辑图、500×400 视口 | 盒体被夹紧到视口余量内且保持 margin | `test app_effect_52_a_small_viewport_clamps_the_box_without_losing_the_margin … ok` | Pass |
| APP-KEY-18 | 长图表面 Select/标注工具、对象命中、busy | 禁选区移动/调整/重选/双击完成；允许创建与对象拖动 | `test app_key_18_long_image_annotation_has_no_selection_or_reselect_gestures … ok` | Pass |

## 端到端稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-29 refinish | 自有夹具完成单帧滚动→`completion_preview shown`→AX 点击 `arcshot.completion.preview`→AX `arcshot.overlay.completion.add_rectangle`→AX `arcshot.overlay.finish` | 恢复同一覆盖层；`AddDefaultRectangle … annotations=1`；重新复制整图带标注并结束；真实剪贴板 PNG 尺寸等于完整 crop、hash 变化 | `mac_e2e_29_refinish=pass preview_click=true annotation=1 whole_image=460x360 clipboard_sha256=122ab4a09eff..46c1dede1b7f bytes=35957->36438 session=ended`；Debug/Release 各一次 | Pass |
| MAC-E2E-29 cancel | 第二次完成→读取剪贴板→AX 点击预览进入标注→Esc 取消 | `CancelAttempt` 结束会话；不再产生 `output_publish`；剪贴板 PNG 逐字节不变 | `mac_e2e_29_cancel=pass clipboard_retention=true copies=1 sha256=aec8c441f5b7 bytes=39955 session=cancelled`；Debug/Release 各一次 | Pass |
| MAC-E2E-26/27（回归） | 单帧/多帧滚动、反向去重、三个固定预览位置、像素真值探针 | 预览出现并固定停留后自动消失；像素逐格真值相等；Enter 完成复制 | `mac_e2e_26=pass … preview=true auto_dismiss=true`、`mac_e2e_26_esc=pass`、`mac_e2e_27_right/left/inner=pass … preview=true auto_dismiss=true`；Debug/Release 各一次 | Pass |
| MAC-E2E-28（#34 集成回归） | 慢速扩展后原子跳变 Gap 停止；放弃与接受两条显式选择 | 放弃返回原选区且无输出；接受后才复制并进入共享整图预览 | `mac_e2e_28_abandon=pass gap_stop=true choices=two incomplete_warn=true abandon=selection_returned output=none`；`mac_e2e_28_accept=pass gap_stop=true choices=two no_auto_output=true accept=clipboard shared_preview=true auto_dismiss=true`；Debug/Release 各一次 | Pass |

## Not tested

- **三秒停留的动画观感与“悬停不延长”的主观计时**：属于 §4.4 用户视觉/人工
  判断项；本票实现了 3 秒固定期限（`COMPLETION_PREVIEW_HOLD`）且没有任何悬停
  处理器延长期限，但未做人工计时验收。
- **TextEdit 固定文档与 Safari `file://` 静态页目标**：I-13 要求三类目标，本票
  只在自有 GPUI 夹具（类 1）验证；另两类保持 **Not tested**。
- **真实 TCC 切换、System Settings 返回重检、运行中撤权**：本机三项权限均已
  授权，改动 TCC 需用户参与（§4.4）。
- **`agent-desktop clipboard-get`**：该版本（1.1.0）报告 packaged helper 缺失
  （`clipboard_helper_invalid`），因此剪贴板回读由脚本内的临时 Swift
  `NSPasteboard` 读取器完成（文本/数值/hash 证据，未保存取证图像）。
- **原生请求错误/超时实机注入、worker panic 实机复现、24M 像素/2 GiB 峰值**：
  只有注入式接缝覆盖，保持 **Not tested**。

## 给下一票的接缝

- 完成预览的后处理 seam 在 `CaptureSession`：`mark_completion_preview(ticket)`
  把当前冻结文档标记为“已验证整图完成”；下一次成功的 Clipboard 输出会发出
  `CaptureEffect::ShowCompletionPreview` 并保持文档，失败则清除标记并恢复采集。
  普通完成与 #34 的 `AcceptScrollPartial` 都调用同一方法，复用预览、标注、
  整图输出与取消保留。
- 激活预览消耗标记（`EnterCompletionAnnotation`）；再完成按普通 Finish 结束
  会话。预览期间的直接 Finish 也被视为重新完成，不二次预览。
- 输出仍走 `OutputJob`/`encode_document`/`publish_output` 单一路线；`output_publish`
  现已打印 `pixels=WxH annotations=N`，可用于任何完成路径的整图断言。
- 长图标注面是同一 `CaptureView` + 同一 `Document`；视图变换只作用于显示和
  指针映射（`ViewTransform`），不回写 crop/selection。长图模式下不提供选区
  手势与滚动入口；`arcshot.overlay.completion.add_rectangle` 是辅助功能非拖拽
  入口，调用同一 `Document::add_rectangle`。
- #34 的 `AwaitingPartialChoice` 与本票预览状态互斥：等待选择时 Start 被忽略，
  接受后才进入共享预览。
