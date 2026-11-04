# #27 Blur Rectangle 三档模糊与层叠编辑验证记录

本记录覆盖 #27 落地的 I-07 Blur Rectangle：**弱 / 中 / 强**三档高斯毛玻璃、
4/8/12 逻辑点半径按 2x 转为物理像素、只模糊矩形内且只模糊已合成的下层、
选区边缘像素延伸、移动/调整重算、创建/编辑/历史/样式记忆，以及覆盖层第五个工具与
三档选项的真实 AX 驱动。证据来自 core 语义动作、固定文档像素真值、app 键位/指针
路由、真实 AX 驱动的 E2E 与完整 `scripts/check-all.sh`。真实指针绘制、模糊对象
命中/控制点拖动、预览观感、tooltip 文本与需要人工授权/判断的项目保持
**Not tested**。本票没有保存用户截图、没有联网、没有请求新权限；E2E 临时切换的
「完成音效」与「选项密度」偏好在同一轮还原。

## 实测来源与环境

- 基线：`spec/v1` = `aa0c31d`（含 #22/#23/#24/#25/#26/#30 与 #29 选项密度）；#27 功能
  提交 `404861c`，与 #29 的合并 `849ceb7`，E2E/`TEST.md`/tooltip 守护提交
  `5c809b3`。
- 实测实现/脚本 SHA：`5c809b3194c625b32f49d951266ed421967ff870`（#27 功能 + 与 #29
  的合并 + E2E 稳定 ID/`TEST.md` + tooltip 守护）；后续提交只增加本证据文档。
- 时间：2026-11-04T08:53:34Z（`cargo test --locked --workspace --all-features` 完成）；
  Debug E2E 与 Release E2E 分别在 2026-11-04 完成。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；均由 `scripts/build-app.sh` 重建，
  本机自签名身份 `ArcShot Dev`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)，`/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk`。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，Target arm64-apple-macosx27.2.0。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏 ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；产品日志
  `capture_request_us=… display=1 pixels=2940x1912 stride=11760 scale=2` 给出物理
  2940×1912；`system_profiler` 报面板 2560×1664 Retina（缩放模式）。未保存屏幕图像。
- 权限：`agent-desktop status` 为 `accessibility=granted`、`screen_recording=granted`、
  `automation=unknown`、`platform=macos`；被测 bundle 的 Screen Recording preflight
  通过（日志真实 `capture_request_us=` 与 `capture_probe_grid`）。未改动 TCC，
  未请求通知权限，未注册/注销登录项（`login_item_status … source=menu/launch` 只读）。
- 夹具：`fixture=blur-e2e-v1`：真实 Debug/Release bundle、core/image 固定像素真值
  合成夹具（80×50 逻辑点 / 160×100 物理像素 @2x 的台阶边、均匀面、边缘列与白块）、
  `agent-desktop 1.1.0`（envelope `2.6`）、真实 `NSUserDefaults` 偏好域
  （完成音效与选项密度切换并还原）与真实 `SMAppService` 只读状态。无图像夹具、
  无外部应用夹具。
- 日志：`target/e2e-agent-desktop-debug.log`、`target/e2e-agent-desktop-release.log`、
  `target/e2e-agent-desktop-debug-restart-1…5.log`、
  `target/e2e-agent-desktop-release-restart-1…5.log`、`target/27-check-all-final.log`。
  日志是本地构建产物，不入库；本报告只保存文本/数值摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本报告是后续纯证据
  文档提交，不改代码或脚本。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test blur
cargo test --locked -p arcshot-core --test session_edit core_session_27
cargo test --locked -p arcshot-image --test blur
cargo test --locked -p arcshot-app --test key_policy app_key_16
cargo test --locked -p arcshot-app --bin arcshot blur_tooltip
scripts/check-all.sh > target/27-check-all-final.log 2>&1
scripts/e2e-agent-desktop.py > target/27-e2e-debug.log 2>&1
scripts/e2e-agent-desktop.py --release > target/27-e2e-release.log 2>&1
```

- `cargo clippy … -D warnings`：exit 0，`Finished dev profile`。
- `cargo test --locked --workspace --all-features`：exit 0，全部测试二进制
  `test result: ok`，共 181 个 Rust 测试。
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
menu_items_smoke=pass count=7 density_index=3
preference_items_smoke=pass density_selector=toggleDensity: sound_selector=toggleCompletionSound: login_selector=toggleLaunchAtLogin: state_from_system=true
login_item_status_smoke=pass state=Disabled read_only=true
preference_smoke=pass key=CompletionSoundEnabled roundtrip=true restored=true
density_smoke=pass key=OptionDensity roundtrip=true restored=true default=Compact
hotkey_registration_smoke=pass default=⌘⇧X suspend_resume_cleanup=clean
clipboard_pngf_smoke=pass width=2 height=2 private_pasteboard=true
save_file_smoke=pass bytes=87 roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
privacy_smoke=pass resident_pid=958 network_sockets=0 lsof_status=1
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
capture_window_exclusion_smoke=pass visible_owned_windows=0 no_native_capture=true
capture_menu_smoke=pass queued_commands=6 menu_items=7 no_native_capture=true
menu_items_smoke=pass count=7 density_index=3
preference_items_smoke=pass density_selector=toggleDensity: sound_selector=toggleCompletionSound: login_selector=toggleLaunchAtLogin: state_from_system=true
login_item_status_smoke=pass state=Disabled read_only=true
preference_smoke=pass key=CompletionSoundEnabled roundtrip=true restored=true
density_smoke=pass key=OptionDensity roundtrip=true restored=true default=Compact
hotkey_registration_smoke=pass default=⌘⇧X suspend_resume_cleanup=clean
clipboard_pngf_smoke=pass width=2 height=2 private_pasteboard=true
save_file_smoke=pass bytes=87 roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示（非 Clippy
  `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug … cases=23`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release … cases=23`。

## 稳定自动化子用例（Rust）

前置为上述 SHA、锁定依赖与固定合成夹具；「实际」列为 `cargo test --locked --workspace
--all-features` 的观察行。所有结果均为 **Pass**。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-EDIT-32 | 有效选区内以 Blur 工具按下—拖动—松开；含越界矩形与零尺寸 | 默认中档；选区内起始；越界保留；零尺寸不入历史；自动选中且工具保持 | `test core_edit_32_the_blur_tool_creates_medium_rectangles_from_inside_the_selection ... ok` | Pass |
| CORE-EDIT-33 | 选中 Blur/其他类型对象各自点击选项；按下时改样式；新会话 | 只改同类型选中对象与工具记忆；按下时样式快照；每会话默认中档 | `test core_edit_33_blur_options_edit_only_same_type_objects_and_remember_per_session ... ok` | Pass |
| CORE-EDIT-34 | 模糊对象主体移动、八控制点越边翻转、删除、选区移动/调整 | 与矩形规则一致；选区调整不缩放对象 | `test core_edit_34_blur_geometry_editing_matches_the_rectangle_rules ... ok` | Pass |
| CORE-SESSION-27 | 经权威会话派发 BlurOption/其他工具选项/输出在途/过期 ticket | 改选中同类对象与工具记忆；其他选项不改；输出在途忽略；过期 ticket 不改新会话 | `test core_session_27_blur_actions_share_the_edit_fence ... ok` | Pass |
| IMAGE-BLUR-19 | 固定台阶边文档，三档各测水平剖面 | 支撑恰好 ±8/±16/±24 物理像素；边缘值 141/134/132；10–90 宽度 10/20/28；过渡连续 | `test image_blur_19_three_levels_use_the_specified_physical_radii ... ok` | Pass |
| IMAGE-BLUR-20 | 均匀 [10,20,30,255] 面，三档 | 每个像素逐字节不变；无色罩、无 alpha 变化 | `test image_blur_20_a_uniform_surface_is_unchanged_with_no_colour_or_alpha_mask ... ok` | Pass |
| IMAGE-BLUR-21 | 台阶边与右边界恰好落在矩形边的矩形 | 矩形内 x=79 为 114/121/123；矩形外 x=80 仍为 255；两端外侧不变 | `test image_blur_21_blur_is_clipped_to_the_rectangle ... ok` | Pass |
| IMAGE-BLUR-22 | 唯一白像素在选区边界 x=0 | 边缘像素延伸使 x=0 为 141/134/132；影响不超过支撑 | `test image_blur_22_the_selection_edge_extends_its_edge_pixels ... ok` | Pass |
| IMAGE-BLUR-23 | 下层红填充矩形 + 模糊 + 上层蓝描边矩形，与无模糊同文档对比 | 下层像素改变；上层蓝描边逐字节等于 `#007AFF`；矩形外不变 | `test image_blur_23_lower_layers_blur_and_later_objects_stay_crisp ... ok` | Pass |
| IMAGE-BLUR-24 | 白块文档上移动并缩放模糊矩形 | 移动/缩放后按新几何重算；离开后白块恢复锐利 | `test image_blur_24_moving_or_resizing_recomputes_at_the_new_geometry ... ok` | Pass |
| IMAGE-BLUR-25 | 同一文档合成两次；`preview_bgra` 与 `encode_png` | 确定性；预览为同一 RGBA 的 BGRA 布局；PNG 解码回同一像素 | `test image_blur_25_the_effect_is_deterministic_and_preview_png_share_pixels ... ok` | Pass |
| APP-KEY-16 | Blur 工具激活时区内按下、区外重选、双击、对象命中、Esc 分层、Delete | 区内创建；区外重选；双击不完成；对象命中仍创建；Esc 分层；Delete 需选中对象 | `test app_key_16_the_blur_tool_shares_the_annotation_pointer_and_esc_priority ... ok` | Pass |
| TOOLTIP-27-01 | `ui::blur_tooltip_tests` 读取 `BLUR_TOOLTIP` | 文本含「模糊不保证敏感信息不可读」与「不是可靠的信息遮盖方式」 | `test ui::blur_tooltip_tests::the_blur_tooltip_states_the_redaction_limitation ... ok` | Pass |

配套的 `crates/core/tests/{objects,session_edit,document,editor,lines}.rs`、
`crates/image/tests/{compose,annotations,lines}.rs`、`crates/app/tests/*.rs` 与
`crates/app` 单元测试在同一完整入口下全部 `ok`（共 181 个 Rust 测试），未因
Blur 对象回归。`AnnotationStyle::stroke()` / `width()` 变为 `Option`，原有
`objects.rs` / `session_edit.rs` / `ui.rs` / `compose.rs` 调用点同步更新。

## E2E 稳定子用例（Debug 与 Release 各 23/23）

命令：`scripts/e2e-agent-desktop.py [--release]`（`agent-desktop 1.1.0`，envelope `2.6`）。
本票新增 **MAC-E2E-23**；#29 的 MAC-E2E-22 与 #30 的 15～17 一并在同一脚本重跑通过。
状态菜单为七项（含 #29 的 选项密度）。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-23 | MAC-E2E-21 后 AX 激活 `arcshot.overlay.tool.blur`，`hover @<blur ref> --headed` 再 `snapshot`，依次 AX 激活 `option.blur.weak` / `.strong` | 五个工具可寻址；`capture_edit action=SelectTool … tool=Blur` 且 checked 互换；三档选项可寻址，默认仅 `medium` checked；`capture_edit action=BlurOption … level=weak/strong`，checked 更新 | `mac_e2e_23=pass tool_dock=ax blur_levels=weak/medium/strong default=medium diagnostics=level` | Pass |
| MAC-E2E-22（#29） | 读 `preferences_launch density=`；冻结后保持 Rectangle 与样式；状态菜单切 选项密度 两次并重启 | `option_density density=… persisted=true`；`arcshot.overlay.density` AX `value` 变化；工具与样式保持；重启后保持；结束时还原 | `mac_e2e_22=pass density=Compact->Standard->Compact option_set=reachable tool_and_style=preserved restart=restored ax=dock_value` | Pass |
| MAC-E2E-18～21（#26） | 四个工具与 Line/Arrow 悬停选项 AX 激活 | 既有行为不回归 | `mac_e2e_18=…`、`mac_e2e_19=…`、`mac_e2e_20=…`、`mac_e2e_21=…` | Pass |
| MAC-E2E-15/16/17（#30） | 完成音效切换/跨两次重启还原/登录项只读 | 既有偏好行为不回归 | `mac_e2e_15=…`、`mac_e2e_16=…`、`mac_e2e_17=…` | Pass |
| MAC-E2E-01～14 | 驻留、捕获、Esc 分层、热键阴性对照、权限提示、快捷键设置、工具坞/矩形选项、覆盖层 AX 取消 | 原行为不回归 | 逐条 `mac_e2e_XX=pass`；结尾 `mac_e2e=pass … cases=23` | Pass |

### 驱动与观察合同

- `agent-desktop --version`：`agent-desktop 1.1.0`；
  `agent-desktop status` 原始输出：`{"version":"2.6","ok":true,"command":"status","data":{"permissions":{"accessibility":{"state":"granted"},"automation":{"state":"unknown"},"screen_recording":{"state":"granted"}},"platform":"macos","ref_count":1,"session_id":null,"supported_surfaces":["window","focused","menu","menubar","sheet","popover","alert","dock","menu-bar-extras"],"tracing":false,"version":"1.1.0"}}`。
- `list-apps --app "ArcShot"`：门禁时为空；启动后 Debug 恰为
  `dev.arcshot.app.debug` / `ArcShot Debug`（`mac_e2e_09=pass`），Release 恰为
  `dev.arcshot.app` / `ArcShot`；退出后为空（`mac_e2e_08=pass exit=0 status_item=removed`）。
- 确切命令序列（由脚本串行执行）：`agent-desktop status` → `list-apps --app "ArcShot"`
  → `scripts/build-app.sh [--release]` → `plutil -extract CFBundleIdentifier/CFBundleName`
  → 启动可执行文件并把 stderr 重定向到 `target/e2e-agent-desktop-<configuration>.log`
  → `snapshot --surface menu-bar-extras --app <name> --compact` →
  `find --surface menu-bar-extras --role menuitem --name <命令>` → `click @ref`
  → `list-windows --app <name>` → `press escape --headed` / `press <组合>`
  → `snapshot --app <name>` → `find_nodes` 命中 `native_id` → `click @ref`
  → `hover @ref --headed` → `wait_log`/`wait_line` 读产品 stderr →
  `click 退出 ArcShot` → 五次 `start`/`quit_app` 重启（#30 两次 + #29 三次）。
- `error.code`：所有 headless `find`/`click`/`snapshot`/`list-windows` 步骤的
  envelope 均为 `ok=True`（脚本 `agent()` 对非 ok 立即 `fail`）；成功步骤逐条打印
  `mac_e2e_XX=pass`。`--headed` 按键按平台语义为 `delivery=delivered_unverified`
  的物理投递，产品是否收到一律以 stderr 诊断行断言。
- 逐条 `a11y_action=arcshot.overlay.tool.blur action=Click` 与
  `a11y_action=arcshot.overlay.option.blur.{weak,strong} action=Click` 证明动作真的
  到达产品；`capture_edit` 证明语义状态；`version=0` 证明工具记忆/样式改动不产生
  文档历史。

### 被引用的 stderr 原文（最终 Debug E2E，`target/e2e-agent-desktop-debug.log`）

```text
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.tool.blur action=Click
level=INFO target=arcshot::coordinator capture_edit action=SelectTool version=0 annotations=0 selection=None tool=Blur selected=None style=level=medium
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.blur.weak action=Click
level=INFO target=arcshot::coordinator capture_edit action=BlurOption version=0 annotations=0 selection=None tool=Blur selected=None style=level=weak
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.blur.strong action=Click
level=INFO target=arcshot::coordinator capture_edit action=BlurOption version=0 annotations=0 selection=None tool=Blur selected=None style=level=strong
```

`style=level=…` 在 `BlurOption` 行反映被修改工具的样式记忆（`action_style_tool`），
`tool=Blur` 反映当时激活的工具；E2E 断言的是 `style=` 内容与 checked 状态。

## 说明与限制

- `AnnotationStyle::stroke()` / `width()` 现在返回 `Option`，因为 Blur 没有描边；
  这是 #27 的 API 变化，原有调用点（`ui.rs` 的对象控制点颜色、`compose.rs` 的
  `draw_arrow`、`objects.rs` / `session_edit.rs` 测试）同步更新。`draw_arrow` 改为
  接收已匹配的 `ArrowStyle`，对象控制点对 Blur 使用白色，均无 panic。
- 模糊算法是归一化 1D 高斯核（`sigma = 半径 / 2`，支撑 = 物理半径）的两次可分离
  卷积，水平中间结果取整到 u8；三档物理半径 8/16/24 由 4/8/12 逻辑点 ×2 得到。
  `IMAGE-BLUR-19` 的剖面值、支撑边界与 10–90 宽度均按独立 Python 参考实现计算后
  硬编码，不是从产品输出反推。
- `agent-desktop 1.1.0` 的 `snapshot` / `find` 不暴露 GPUI tooltip 或
  `aria_description`（已用 `find --text` / `--description` 与 hover 后 snapshot 验证
  均无该文本）。因此 tooltip 文本由 `ui::blur_tooltip_tests` 守护，E2E 只断言 AX id
  与 `capture_edit` 诊断；这是工具版本限制，不是产品缺口。
- E2E 会临时写入 Debug/Release 的完成音效与选项密度偏好并在同一轮还原；真实登录项
  从未注册/注销（`login_item_status` 全程只读，且无 `login_item_set`）。
- 像素真值由固定文档的 `image/tests/blur.rs` 证明；预览与最终输出共用
  `compose_document`，没有第二条合成路径。`--headed` 坐标点击只作为悬停浮现选项的
  前置，激活证据一律是 AX 行与产品诊断行；没有保存任何截图或取证图像。

## 仍为 Not tested 的项目

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-ANNOTATION-POINTER-27-01 | 真实指针在选区内按下—拖动—松开创建 Blur Rectangle | 按 I-07 创建、自动选中、工具保持激活、零尺寸拒绝 | 未执行：对象绘制只有指针手势，无 AX 节点；`--headed` 坐标拖拽不作验收证据 | Not tested |
| MAC-ANNOTATION-POINTER-27-02 | 真实指针拖动模糊主体、八控制点、删除、Cmd+Z 撤销 | 整体移动/八向调整/删除/撤销符合 CORE-EDIT-34 | 未执行：对象与控制点不是 AX 节点，E2E 无法驱动 | Not tested |
| MAC-ANNOTATION-VISUAL-27-03 | 在真实屏幕上肉眼比较三档模糊与层叠效果 | 弱/中/强可区分；下层模糊、上层清晰；矩形边界清晰 | 未执行：视觉/审美判断属 §4.4，自动化只证明像素真值与几何剖面 | Not tested |
| MAC-ANNOTATION-VISUAL-27-04 | 比较冻结预览与最终 PNG 的模糊外观 | 预览与输出一致 | 未执行：需保存/比较取证图像；代码层共用同一合成器，不等于实机观感 | Not tested |
| MAC-ANNOTATION-NATIVE-27-05 | 选区边缘的模糊、越界模糊矩形、移动/调整选区后的裁剪显示 | 边缘像素延伸；越界不显示；扩大选区重现且对象不缩放 | 未执行：需真实指针与屏幕像素观察，core/image 已覆盖语义 | Not tested |
| MAC-ANNOTATION-TOOLTIP-27-06 | 读 GUI tooltip / `aria_description` 文本 | 明确说明模糊不保证敏感信息不可读、不是可靠脱敏 | 未执行：`agent-desktop 1.1.0` 不暴露 tooltip/description；文本由 `ui::blur_tooltip_tests` 守护 | Not tested |
| MAC-PERMISSION-NATIVE-27-07 | 真实 TCC 授权/撤销、权限提示变体、运行中撤权 | 引导与恢复符合 I-04/I-12 | 未执行：需用户在系统设置授权（§4.4），E2E 只跑已授权状态 | Not tested |
| MAC-START-NATIVE-27-08 | 真实登录项注册/注销、热键冲突恢复、跨注销重启 | 系统实际状态与菜单一致 | 未执行：会改动用户系统状态（#30 亦记为 Not tested） | Not tested |
| PERF/RESOURCE-27-09 | Release 下三档模糊的性能与资源采样 | 达到 T-04 门槛 | 未执行：本票未做性能/资源采样，完整入口只含 Debug+Release 冒烟 | Not tested |
