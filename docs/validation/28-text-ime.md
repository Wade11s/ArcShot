# #28 Text 多行编辑、中文 IME 与栅格化输出验证记录

本记录覆盖 #28 落地的 I-08 Text：左侧第六个工具、固定色板与 6–64 点 0.5 步进字号、
多行不换行编辑、Enter 换行、Esc 结束编辑保留内容、单击别处/切工具提交、双击重编、
空文本丢弃/编辑变空删除且可撤销、创建一步/重编一步的历史粒度，以及共享 `image` 合成器
里可复现字体输入的栅格化（字号/颜色/裁剪/位于模糊层之上）。证据来自 core 语义动作、
固定文档像素真值、app 键位/指针路由、真实 AX 驱动的 E2E 与完整 `scripts/check-all.sh`。
真实指针文字创建/拖动、原生系统字体与 IME 候选窗/焦点、直接输入字号与预览观感仍属
MAC-TEXT，保持 **Not tested**。本票没有保存用户截图、没有联网、没有请求新权限；
E2E 临时切换的「完成音效」与「选项密度」偏好在同一轮还原。

## 实测来源与环境

- 基线：`spec/v1` = `ed74641`（含 #22/#23/#24/#25/#26/#27/#29/#30）；#28 功能提交
  `d86495e`，文字输入 AX 修复 `a1d5f14`，与 #31 的合并 `0fe761e`。
- 实测实现/脚本 SHA：`0fe761ede9b88580b61371185b11ed97d7b1d964`（#28 Text 切片 +
  与 #31 scroll entry 的合并 + `TEST.md`/`scripts/e2e-agent-desktop.py`）；本报告是
  后续纯证据文档提交，不改代码或脚本。
- 时间：2026-11-04T11:20:00Z（`scripts/check-all.sh` 与 Debug/Release E2E 完成）。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；均由 `scripts/build-app.sh` 重建，本机
  自签名身份 `ArcShot Dev`。
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
  通过（日志真实 `capture_request_us=` 与 `capture_probe_grid`）。未改动 TCC，未请求
  通知权限，未注册/注销登录项（`login_item_status … source=menu/launch` 只读）。
- 夹具：`fixture=text-e2e-v1`：真实 Debug/Release bundle、core/image 固定像素真值
  合成夹具（80×50 逻辑点 / 160×100 物理像素 @2x 的台阶边、均匀面、边缘列与白块）、
  `agent-desktop 1.1.0`（envelope `2.6`）、真实 `NSUserDefaults` 偏好域（完成音效与
  选项密度切换并还原）与真实 `SMAppService` 只读状态。无图像夹具、无外部应用夹具。
- 日志：`target/e2e-agent-desktop-debug.log`、`target/e2e-agent-desktop-release.log`、
  `target/28-check-all-final.log`。日志是本地构建产物，不入库；本报告只保存文本/数值摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本报告是后续纯证据
  文档提交，不改代码或脚本。

## 执行

```sh
cargo fmt --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test text
cargo test --locked -p arcshot-image --test text
cargo test --locked -p arcshot-app --test key_policy app_key_17
scripts/check-all.sh > target/28-check-all-final.log 2>&1
scripts/e2e-agent-desktop.py > target/28-e2e-debug.log 2>&1
scripts/e2e-agent-desktop.py --release > target/28-e2e-release.log 2>&1
```

- `cargo clippy … -D warnings`：exit 0，`Finished dev profile`。
- `cargo test --locked --workspace --all-features`：exit 0，41 个测试二进制全部
  `test result: ok`，共 221 个 Rust 测试。
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
privacy_smoke=pass resident_pid=19506 network_sockets=0 lsof_status=1
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示（非 Clippy
  `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，
  `mac_e2e=pass configuration=debug … cases=25`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，
  `mac_e2e=pass configuration=release … cases=25`。

## 稳定自动化子用例（Rust）

前置为上述 SHA、锁定依赖与固定合成夹具；「实际」列为 `cargo test --locked --workspace
--all-features` 的观察行。所有结果均为 **Pass**。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-EDIT-35 | 文字工具在选区内单击进入编辑；空/仅空格换行提交；输入内容后提交 | 空文本不保留且不入历史；真实内容一步创建、自动选中、工具保持 | `test core_edit_35_text_creation_is_one_step_and_empty_content_is_dropped ... ok` | Pass |
| CORE-EDIT-36 | 选择工具双击已有文字重编；改内容一步；编辑变空删除 | 重编一步修改；变空一步删除；撤销恢复 | `test core_edit_36_re_editing_is_one_modification_and_empty_deletes ... ok` | Pass |
| CORE-EDIT-37 | `TextStyle::quantize_size` 边界；选项改选中文字与工具记忆 | 6–64 点、0.5 取整；只改同类；每会话记忆 | `test core_edit_37_text_size_is_clamped_and_quantized ... ok` | Pass |
| CORE-EDIT-38 | 文字命中、层叠、选择工具移动、选区移动携带 | 最上层命中；整体移动一步；选区移动携带；无控制点 | `test core_edit_38_text_hits_moves_and_stacks ... ok` | Pass |
| CORE-EDIT-39 | 编辑中切换工具 | 先提交待编文字再切换，一步历史 | `test core_edit_39_a_text_edit_commits_when_the_tool_changes ... ok` | Pass |
| CORE-SESSION-44 | 经权威会话派发 TextBegin/TextEdited/TextCommit/TextOption；Finish 先提交；输出在途忽略 | 草稿不进注释直到提交；Finish 快照含已提交文字；输出在途忽略编辑 | `test core_session_44_text_actions_share_the_edit_fence ... ok` | Pass |
| CORE-SESSION-45 | TextCancel 丢弃；过期 ticket 派发文字动作 | 取消不产生历史；过期 ticket 不改新会话 | `test core_session_45_text_cancel_discards_and_stale_tickets_never_edit ... ok` | Pass |
| IMAGE-COMPOSE-26 | 固定 5x7 字形 'A' 在 16pt/2x 的物理像素 | 顶横 12×4、横杠全宽、基线 28；确定性 | `test image_compose_26_text_uses_the_documented_reproducible_glyph ... ok` | Pass |
| IMAGE-COMPOSE-27 | 32pt 与蓝色 | 字号整对象翻倍；颜色逐像素生效 | `test image_compose_27_text_size_and_colour_apply_to_the_whole_object ... ok` | Pass |
| IMAGE-COMPOSE-28 | 缩小选区左边界 | 越界文字像素被裁剪，不重采样 | `test image_compose_28_text_is_clipped_to_the_selection ... ok` | Pass |
| IMAGE-COMPOSE-29 | 文字在模糊层之上 | 文字像素逐字节为样式色；背景被模糊 | `test image_compose_29_text_layers_above_a_blur_object ... ok` | Pass |
| IMAGE-COMPOSE-30 | 多行与未知字形 | 行高确定；未知字形为固定空心框 | `test image_compose_30_multiline_and_unknown_glyphs_are_deterministic ... ok` | Pass |
| IMAGE-COMPOSE-31 | 自定义字体输入 | 合成器使用传入字体，默认入口不受影响 | `test image_compose_31_the_font_input_is_pluggable ... ok` | Pass |
| IMAGE-COMPOSE-32 | 预览 BGRA 与 PNG 解码 | 预览为同一 RGBA 的 BGRA；PNG 解码回同一像素 | `test image_compose_32_text_preview_and_png_share_pixels ... ok` | Pass |
| IMAGE-COMPOSE-33 | 逻辑度量与物理位图 | 行数/行高/最大行宽与位图尺寸一致 | `test image_compose_33_logical_metrics_match_the_physical_bitmap ... ok` | Pass |
| IMAGE-COMPOSE-34 | 文字几何保留内容与裁剪边界 | 平移保留内容；裁剪交集正确 | `test image_compose_34_text_geometry_retains_content_and_clip_bounds ... ok` | Pass |
| APP-KEY-17 | 编辑期间 Enter/Delete/Backspace/撤销/重做、Esc、拖动/输出期、双击文字 | 编辑键归编辑器/IME；Esc 先结束编辑；拖动/输出优先；双击文字重编 | `test app_key_17_text_editing_consumes_editor_keys_and_esc_ends_it_first ... ok` | Pass |

配套的 `crates/core/tests/{objects,session_edit,document,editor,lines,blur,scroll}.rs`、
`crates/image/tests/{compose,annotations,lines,blur,text}.rs`、`crates/app/tests/*.rs`
与 `crates/app` 单元测试在同一完整入口下全部 `ok`（共 221 个 Rust 测试），未因 Text
对象回归。`AnnotationStyle::stroke()` 对 Text 返回 `Some(color)`，`width()` 对 Text
返回 `None`；`Shape` 新增 `Text` 变体后 `Annotation`/`Shape` 不再 `Copy`，原有调用点
改为借用/克隆。

## E2E 稳定子用例（Debug 与 Release 各 25/25）

命令：`scripts/e2e-agent-desktop.py [--release]`（`agent-desktop 1.1.0`，envelope `2.6`）。
本票新增 **MAC-E2E-25**；#31 的 MAC-E2E-24 与 #29/#30 的既有用例一并在同一脚本重跑通过。
状态菜单为七项（含 #29 的 选项密度）。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-25 | MAC-E2E-23 后 AX 激活 `arcshot.overlay.tool.text`，`--headed` 拖拽建选区并点击创建文字（前置），`press a --headed` 输入一个字符，`hover @<text ref> --headed` 再 `snapshot` | 六个工具可寻址；`capture_edit action=SelectTool … tool=Text` 且 checked 互换；文字输入 `arcshot.overlay.text.input` 以 `textfield` 可寻址；选项条按 `arcshot.overlay.option.text.*` 可寻址，默认字号 `16`；AX 激活 `size.up` 后 `capture_edit action=TextOption … size=16.5` 且字号值变为 `16.5`；AX 激活 `color.blue` 后 `stroke=#007AFF` 且 checked 更新；点击别处出现 `capture_edit action=TextCommit … annotations=1` | `mac_e2e_25=pass tool_dock=ax text_editor=ax size=16->16.5 colour=#007AFF commit=1` | Pass |
| MAC-E2E-24（#31） | 开始截图冻结后 AX 点击 `arcshot.overlay.scroll`（无选区），`--headed` 拖拽建选区，再 AX 激活入口 | 无选区 `POLICY_DENIED`；拖拽后 `SelectRegion`；入口点击出现 `scroll_permission` 诊断 | `mac_e2e_24=pass entry_gate=ax selection_precondition=headed_drag missing=none` | Pass |
| MAC-E2E-01～23 | 驻留、捕获、Esc 分层、热键阴性对照、权限提示、快捷键设置、工具坞/矩形/直线/箭头/模糊选项、覆盖层 AX 取消、完成音效/密度/登录项 | 既有行为不回归 | 逐条 `mac_e2e_XX=pass`；结尾 `mac_e2e=pass … cases=25` | Pass |

### 驱动与观察合同

- `agent-desktop --version`：`agent-desktop 1.1.0`；
  `agent-desktop status` 原始输出：`{"version":"2.6","ok":true,"command":"status","data":{"permissions":{"accessibility":{"state":"granted"},"automation":{"state":"unknown"},"screen_recording":{"state":"granted"}},"platform":"macos","ref_count":9,"session_id":null,"supported_surfaces":["window","focused","menu","menubar","sheet","popover","alert","dock","menu-bar-extras"],"tracing":false,"version":"1.1.0"}}`。
- `list-apps --app "ArcShot"`：门禁时为空；启动后 Debug 恰为
  `dev.arcshot.app.debug` / `ArcShot Debug`（`mac_e2e_09=pass`），Release 恰为
  `dev.arcshot.app` / `ArcShot`；退出后为空（`mac_e2e_08=pass exit=0 status_item=removed`）。
- 确切命令序列（由脚本串行执行）：`agent-desktop status` → `list-apps --app "ArcShot"`
  → `scripts/build-app.sh [--release]` → `plutil -extract CFBundleIdentifier/CFBundleName`
  → 启动可执行文件并把 stderr 重定向到 `target/e2e-agent-desktop-<configuration>.log` →
  `snapshot --surface menu-bar-extras --app <name> --compact` →
  `find --surface menu-bar-extras --role menuitem --name <命令>` → `click @ref` →
  `list-windows --app <name>` → `press escape --headed` / `press <组合>` →
  `snapshot --app <name>` → `find_nodes` 命中 `native_id` → `click @ref` →
  `hover @ref --headed` → `wait_log`/`wait_line` 读产品 stderr →
  `click 退出 ArcShot` → 五次 `start`/`quit_app` 重启（#30 两次 + #29 三次）。
- `error.code`：所有 headless `find`/`click`/`snapshot`/`list-windows` 步骤的
  envelope 均为 `ok=True`（脚本 `agent()` 对非 ok 立即 `fail`）；成功步骤逐条打印
  `mac_e2e_XX=pass`。`--headed` 按键/拖拽按平台语义为 `delivery=delivered_unverified`
  的物理投递，产品是否收到一律以 stderr 诊断行断言。
- 逐条 `a11y_action=arcshot.overlay.tool.text action=Click`、
  `a11y_action=arcshot.overlay.option.text.size.up action=Click` 与
  `a11y_action=arcshot.overlay.option.text.color.blue action=Click` 证明动作真的到达
  产品；`capture_edit` 证明语义状态；`version=1` 证明工具记忆/样式改动不产生文档历史，
  `TextCommit version=2 annotations=1` 证明一次创建。

### 被引用的 stderr 原文（最终 Debug E2E，`target/e2e-agent-desktop-debug.log`）

```text
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.tool.text action=Click
level=INFO target=arcshot::coordinator capture_edit action=SelectRegion version=1 annotations=0 selection=Some([50.0, 17.0, 150.0, 150.0]) tool=Text selected=None style=stroke=#FF3B30 size=16
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.text.size.up action=Click
level=INFO target=arcshot::coordinator capture_edit action=TextOption version=1 annotations=0 selection=Some([50.0, 17.0, 150.0, 150.0]) tool=Text selected=None style=stroke=#FF3B30 size=16.5
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.text.color.blue action=Click
level=INFO target=arcshot::coordinator capture_edit action=TextOption version=1 annotations=0 selection=Some([50.0, 17.0, 150.0, 150.0]) tool=Text selected=None style=stroke=#007AFF size=16.5
level=INFO target=arcshot::coordinator capture_edit action=TextCommit version=2 annotations=1 selection=Some([50.0, 17.0, 150.0, 150.0]) tool=Text selected=Some(0) style=stroke=#007AFF size=16.5
```

`style=stroke=… size=…` 在 `TextOption` 行反映被修改工具的样式记忆（`action_style_tool`），
`tool=Text` 反映当时激活的工具；E2E 断言的是 `style=` 内容与 checked 状态。

## 说明与限制

- `Annotation`/`Shape` 不再 `Copy`：`Shape::Text` 携带 `String` 内容。原有调用点改为
  借用/克隆；`UiCommand` 也因 `TextEdited` 携带 `String` 而失去 `Copy`，所有按钮闭包
  改为按闭包克隆命令。#31 的滚动按钮在同一合并中同步修复。
- 文字栅格化由 `image::TextFont` 输入驱动，`BitmapFont` 是固定的 5x7 等宽字形（未知
  字形为空心框），保证 golden 确定性；`compose_document` 默认使用它，预览与两种 PNG
  输出共用同一合成器。原生 macOS 系统字体/IME 路径不在本票自动化范围，见下。
- 编辑器的 GPUI `ElementInputHandler` 通过 `track_focus` 暴露为 AX `textfield`
  （`arcshot.overlay.text.input`），并将内容变化实时推送为 `TextEdited`；Finish/Save
  与 Esc/单击别处经 `Document::commit_edits()` 一步提交。E2E 用 `press a --headed`
  作为输入前置，IME 组合文字本身另属 MAC-TEXT。
- E2E 会临时写入 Debug/Release 的完成音效与选项密度偏好并在同一轮还原；真实登录项
  从未注册/注销（`login_item_status` 全程只读，且无 `login_item_set`）。
- 像素真值由固定文档的 `image/tests/text.rs` 证明；预览与最终输出共用
  `compose_document`，没有第二条合成路径。`--headed` 坐标点击/拖拽只作为悬停浮现
  选项、创建选区/文字与提交的前置，激活证据一律是 AX 行与产品诊断行；没有保存任何
  截图或取证图像。

## 仍为 Not tested 的项目

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-TEXT-28-01 | 真实 macOS 系统默认无衬线字体渲染中文/emoji/多行 | 系统字体、左对齐、默认红 16 点 | 未执行：本票使用可复现 5x7 字体输入，系统字体适配为后续 MAC-TEXT 项 | Not tested |
| MAC-TEXT-28-02 | 中文 IME 组合、候选窗位置、Enter/Esc/提交/Finish/保存不丢组合文字 | 组合文字不丢、候选窗正确 | 未执行：需真实第三方输入法与候选窗观察；E2E 只验证物理键输入与提交诊断 | Not tested |
| MAC-TEXT-28-03 | 真实指针创建/拖动/双击重编文字、删除撤销 | 指针手势符合 I-08 | 未执行：对象无 AX 节点，`--headed` 坐标拖拽不作验收证据 | Not tested |
| MAC-TEXT-28-04 | 直接输入字号（点击字号框键入数字） | 输入/失焦约束并 0.5 取整 | 未执行：自动化入口为 AX 步进与 core 取整测试；直接输入为 UI 实现，未单独原生验证 | Not tested |
| MAC-TEXT-28-05 | 冻结预览与最终 PNG 的文字观感 | 预览与输出一致 | 未执行：需保存/比较取证图像；代码层共用同一合成器，不等于实机观感 | Not tested |
| MAC-ANNOTATION-POINTER-28-06 | 文字越界保留/裁剪/扩大选区重现 | 越界数据保留、裁剪显示 | 未执行：需真实指针与屏幕像素观察，core/image 已覆盖语义 | Not tested |
| MAC-PERMISSION-NATIVE-28-07 | 真实 TCC 授权/撤销、权限提示变体、运行中撤权 | 引导与恢复符合 I-04/I-12 | 未执行：需用户在系统设置授权（§4.4），E2E 只跑已授权状态 | Not tested |
| MAC-START-NATIVE-28-08 | 真实登录项注册/注销、热键冲突恢复、跨注销重启 | 系统实际状态与菜单一致 | 未执行：会改动用户系统状态（#30 亦记为 Not tested） | Not tested |
| PERF/RESOURCE-28-09 | Release 下文字栅格化的性能与资源采样 | 达到 T-04 门槛 | 未执行：本票未做性能/资源采样，完整入口只含 Debug+Release 冒烟 | Not tested |
