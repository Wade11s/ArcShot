# #26 Line / Arrow 线型与端点编辑验证记录

本记录覆盖 #26 落地的 I-06 Line / Arrow 工具：**Line（Solid / Dash）**与
**Arrow（Flat / Inward）**的创建、命中、端点编辑、样式记忆、撤销/删除/裁剪，以及
共享 `image` 合成器里的像素真值。证据来自 core 语义动作、固定文档像素真值、app
键位/指针路由、真实 AX 驱动的 E2E 与完整 `scripts/check-all.sh`。真实指针绘制、
端点拖动、虚线/箭头观感与需要人工授权/判断的项目保持 **Not tested**。本票没有
保存用户截图、没有联网、没有请求新权限；E2E 临时切换的「完成音效」偏好在同一轮还原。

## 实测来源与环境

- 基线：`spec/v1` = `c76a8e4`（含 #22/#23/#24/#25 与 #30 偏好/登录项切片）；#26 功能提交
  `8d95f37`，合并 `432ebfc`，最终代码提交 `6e61b36`。
- 实测实现/脚本 SHA：`6e61b3668c748f50a1b5b0bd5d902c66c5cd9e5f`（#26 功能 +
  与 #30 的合并 + E2E 稳定 ID/焦点重投 + `TEST.md`）；后续提交只增加本证据文档。
- 时间：2026-11-04T07:03:06Z（`scripts/check-all.sh` 完成）与
  2026-11-04T07:12:21Z（Debug / Release E2E 完成，含两次重启）。
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
- 夹具：`fixture=line-arrow-e2e-v1`：真实 Debug/Release bundle、core/image 固定像素真值
  合成夹具、`agent-desktop 1.1.0`（envelope `2.6`）、真实 `NSUserDefaults` 偏好域
  （完成音效切换两次并还原）与真实 `SMAppService` 只读状态。无图像夹具、无外部应用夹具。
- 日志：`target/26-check-all-final.log`、`target/26-e2e-final-debug.log`、
  `target/26-e2e-final-release.log`、`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-release.log`、
  `target/e2e-agent-desktop-debug-restart-1.log` / `-restart-2.log`。日志是本地构建产物，
  不入库；本报告只保存文本/数值摘要。
- 实测时产品源码、`scripts/` 与 `TEST.md` 与上述 SHA 完全一致；本报告是后续纯证据
  文档提交，不改代码或脚本。

## 执行

```sh
cargo fmt --all
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-features
cargo test --locked -p arcshot-core --test session_edit core_session_26
scripts/check-all.sh > target/26-check-all-final.log 2>&1
scripts/e2e-agent-desktop.py > target/26-e2e-final-debug.log 2>&1
scripts/e2e-agent-desktop.py --release > target/26-e2e-final-release.log 2>&1
```

- `cargo check --locked --workspace --all-targets`：exit 0，`Finished dev profile`。
- `cargo test --locked --workspace --all-features`：exit 0，全部测试二进制
  `test result: ok`，共 160 个 Rust 测试。
- `scripts/check-all.sh`：exit 0；关键行（Debug 与 Release 各一份原生冒烟）：

```text
validation_reports=pass directory=docs/validation
dependency_inspection=pass
architecture check passed: app -> core/image/macos; image/macos -> core; core has no product dependencies
text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px
bundle_identity_smoke=pass configuration=debug identifier=dev.arcshot.app.debug name=ArcShot Debug
capture_menu_smoke=pass queued_commands=5 no_native_capture=true
preference_items_smoke=pass sound_selector=toggleCompletionSound: login_selector=toggleLaunchAtLogin: state_from_system=true
login_item_status_smoke=pass state=Disabled read_only=true
clipboard_pngf_smoke=pass width=2 height=2 private_pasteboard=true
save_file_smoke=pass bytes=87 roundtrip=true rejected_destination=true invalid_bytes=true panel_configured=true
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
privacy_smoke=pass resident_pid=72571 network_sockets=0 lsof_status=1
bundle_identity_smoke=pass configuration=release identifier=dev.arcshot.app name=ArcShot
startup_smoke=pass activation_policy=Accessory quit_action=terminate:
```

  链接期有既有的 `ld: duplicate -rpath '/usr/lib/swift' ignored` 提示（非 Clippy
  `-D warnings` 失败项）；完整入口 exit 0。

- `scripts/e2e-agent-desktop.py`（Debug）：exit 0，`mac_e2e=pass configuration=debug … cases=21`。
- `scripts/e2e-agent-desktop.py --release`（Release）：exit 0，`mac_e2e=pass configuration=release … cases=21`。
  另在同一最终脚本上连续跑过 3 次 Debug（cases=21 全 pass）与 2 次 Release
  （cases=21 全 pass），结果一致。

## 稳定自动化子用例（Rust）

前置为上述 SHA、锁定依赖与固定合成夹具；「实际」列为 `cargo test --locked --workspace
--all-features` 的观察行。所有结果均为 **Pass**。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-EDIT-25 | 有效选区内以 Line 工具按下—拖动—松开 | 创建有向线段，方向从起点尾部到终点尖端；选区内起始 | `test core_edit_25_the_line_tool_creates_directed_segments_from_inside_the_selection ... ok` | Pass |
| CORE-EDIT-26 | 以 Arrow 工具拖动，含零长度拖动 | 创建 Flat 箭头；新建零长度被拒绝，非零水平/垂直线有效 | `test core_edit_26_the_arrow_tool_creates_flat_arrows_and_rejects_zero_length ... ok` | Pass |
| CORE-EDIT-27 | 叠放两条细线，在 2 点描边附近命中 | 最上层对象命中；细线有合理命中容差 | `test core_edit_27_hit_testing_prefers_the_topmost_line_and_tolerates_thin_strokes ... ok` | Pass |
| CORE-EDIT-28 | 拖动主体、拖动两端点至重合 | 主体整体移动；端点改长度/方向；既有对象可重合不删除 | `test core_edit_28_endpoints_edit_length_and_direction_and_may_coincide ... ok` | Pass |
| CORE-EDIT-29 | 选中 Line/Arrow/Rectangle 各自点击三类选项 | 只改同类型选中对象与对应工具记忆；颜色/线宽/虚实/内凹矩阵 | `test core_edit_29_line_and_arrow_options_edit_only_same_type_objects ... ok` | Pass |
| CORE-EDIT-30 | 按下时改工具样式；新会话 | 创建保留按下时的样式快照；每会话样式恢复默认 | `test core_edit_30_creation_keeps_the_press_style_and_new_sessions_reset_defaults ... ok` | Pass |
| CORE-EDIT-31 | 移动/调整选区后检查线段端点 | 相对几何保持；调整选区不缩放标注 | `test core_edit_31_selection_move_carries_line_endpoints_and_resize_never_scales_them ... ok` | Pass |
| CORE-SESSION-26 | 经权威会话派发 LineOption/ArrowOption/输出在途 | 改选中同类对象与工具记忆；输出在途忽略；版本栅栏一致 | `test core_session_26_line_and_arrow_actions_share_the_edit_fence ... ok` | Pass |
| IMAGE-COMPOSE-12 | 固定文档，2/3/5 点实线 | 物理线宽随逻辑点×scale，单次换算 | `test image_compose_12_solid_lines_use_the_stroke_width_in_physical_pixels ... ok` | Pass |
| IMAGE-COMPOSE-13 | 固定文档，2/3/5 点虚线 | 虚线实段/间距随线宽缩放，不锁死像素 | `test image_compose_13_the_dash_pattern_scales_with_the_stroke_width ... ok` | Pass |
| IMAGE-COMPOSE-14 | Flat 箭头像素横截面 | 等粗线身、平尾、实心三角头 | `test image_compose_14_flat_arrows_keep_a_constant_body_and_a_filled_triangle_head ... ok` | Pass |
| IMAGE-COMPOSE-15 | Inward 箭头三带横截面 | 尖细尾渐宽、头部后缘内凹，不是双向内指箭头 | `test image_compose_15_inward_arrows_taper_from_a_thin_tail_into_a_concave_head ... ok` | Pass |
| IMAGE-COMPOSE-16 | 线段/箭头部分越界 | 裁剪到选区 crop，越界数据不显示/不输出 | `test image_compose_16_lines_and_arrows_clip_to_the_selection_crop ... ok` | Pass |
| IMAGE-COMPOSE-17 | 同一文档不同 crop | 任意 crop 与完整合成得到相同标注像素 | `test image_compose_17_any_crop_composes_the_same_line_and_arrow_pixels ... ok` | Pass |
| IMAGE-COMPOSE-18 | 2/3/5 点 Flat 与 Inward | 每种线宽两种头都视觉可区分；固定色板 | `test image_compose_18_both_arrow_heads_stay_distinguishable_at_every_width ... ok` | Pass |
| APP-KEY-13 | Line/Arrow 工具激活时区内按下、区外重选、双击 | 区内创建对象；区外重选；标注工具内双击不完成 | `test app_key_13_line_and_arrow_tools_create_inside_and_never_finish_on_double_click ... ok` | Pass |
| APP-KEY-14 | Esc 分层、输出在途取消 | 对象→工具→会话分层；输出期 Esc 仍取消 | `test app_key_14_escape_layers_line_and_arrow_objects_then_tools_then_session ... ok` | Pass |
| APP-KEY-15 | 选择工具与标注工具下命中同一对象 | 对象命中只在 Select 工具下是对象手势 | `test app_key_15_a_line_object_hit_is_an_object_gesture_only_with_the_select_tool ... ok` | Pass |

配套的 `crates/core/tests/editor.rs`、`objects.rs`、`session_edit.rs`、
`crates/image/tests/annotations.rs` 与 `crates/app/tests/key_policy.rs` 既有子用例
在同一完整入口下全部 `ok`（共 160 个 Rust 测试），未因线/箭头对象回归。

## E2E 稳定子用例（Debug 与 Release 各 21/21）

命令：`scripts/e2e-agent-desktop.py [--release]`（`agent-desktop 1.1.0`，envelope `2.6`）。
本票新增四例；其余 01～17 为既有/合并自 #30 的用例，本轮全部重跑通过。
与 #30 合并后状态菜单为六项；Line/Arrow 用例从原先的 15～18 改为
**MAC-E2E-18～21**，把 15～17 留给 #30 的偏好用例。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-18 | 冻结覆盖层 `snapshot --app` 后 AX 激活 `arcshot.overlay.tool.line` 与 `.arrow` | 四工具可寻址；`a11y_action=` 与 `capture_edit action=SelectTool … tool=Line/Arrow`；checked 互换 | `mac_e2e_18=pass tool_dock=ax tools=Line,Arrow selected_state=toggled` | Pass |
| MAC-E2E-19 | `hover @<line ref> --headed` 后 AX 激活 `option.line.dash`、`width.2`、`color.green` | 日志依次 `dash=true`、`width=2`、`stroke=#34C759 width=2 dash=true`；版本不变 | `mac_e2e_19=pass hover_options=ax line_style=#34C759/2/dash version_unchanged` | Pass |
| MAC-E2E-20 | `hover @<arrow ref> --headed` 后 AX 激活 `option.arrow.inward`、`width.5`、`color.orange` | 日志依次 `inward=true`、`width=5`、`stroke=#FF9500 width=5 inward=true`；版本不变 | `mac_e2e_20=pass hover_options=ax arrow_style=#FF9500/5/inward version_unchanged` | Pass |
| MAC-E2E-21 | 19/20 后悬停 Rectangle 工具 | Rectangle 仍显示自己的 5 点/蓝色 checked，证明每工具样式记忆独立 | `mac_e2e_21=pass tool_memory=independent rectangle=#007AFF/5` | Pass |
| MAC-E2E-01（六项菜单） | 启动后 `snapshot --surface menu-bar-extras --app <name>` | 六个菜单项，`native_id` 为 selector | `mac_e2e_01=pass menu_items=6 refs=7` | Pass |
| MAC-E2E-13/14（#25） | 工具坞与 Rectangle 悬停选项 AX 激活 | 既有矩形行为不回归 | `mac_e2e_13=pass …`、`mac_e2e_14=pass …` | Pass |
| MAC-E2E-15/16/17（#30） | 完成音效切换/跨两次重启还原/登录项只读 | 既有偏好行为不回归 | `mac_e2e_15=pass …`、`mac_e2e_16=pass …`、`mac_e2e_17=pass …` | Pass |
| MAC-E2E-02～12 | 驻留、捕获、Esc 分层、热键阴性对照、权限提示、快捷键设置、覆盖层 AX 取消 | 原行为不回归 | 逐条 `mac_e2e_XX=pass`；结尾 `mac_e2e=pass … cases=21` | Pass |

### 驱动与观察合同

- `agent-desktop --version`：`agent-desktop 1.1.0`；
  `agent-desktop status` 原始输出：`{"version":"2.6","ok":true,"command":"status","data":{"permissions":{"accessibility":{"state":"granted"},"automation":{"state":"unknown"},"screen_recording":{"state":"granted"}},"platform":"macos","ref_count":3,"session_id":null,"supported_surfaces":["window","focused","menu","menubar","sheet","popover","alert","dock","menu-bar-extras"],"tracing":false,"version":"1.1.0"}}`。
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
  `click 退出 ArcShot` → 两次 `start`/`quit_app` 重启。
- `error.code`：所有 headless `find`/`click`/`snapshot`/`list-windows` 步骤的
  envelope 均为 `ok=True`（脚本 `agent()` 对非 ok 立即 `fail`）；成功步骤逐条打印
  `mac_e2e_XX=pass`，等价于 `error.code=ok`。`--headed` 按键按平台语义为
  `delivery=delivered_unverified` 的物理投递，产品是否收到一律以 stderr 诊断行断言。
  窗口内按键改用 `--headed`，并在焦点竞态下最多重投 4 次、每次重确认前台；未按到
  仍为非零失败，断言不放宽。
- 逐条 `a11y_action=arcshot.overlay.tool.* action=Click` 与
  `a11y_action=arcshot.overlay.option.{line,arrow}.* action=Click` 证明动作真的到达产品；
  `capture_edit` 证明语义状态；`version=0` 证明工具记忆/样式改动不产生文档历史。

### 被引用的 stderr 原文（最终 Debug E2E，`target/e2e-agent-desktop-debug.log`）

```text
level=INFO target=arcshot::coordinator capture_request_us=142929 display=1 pixels=2940x1912 stride=11760 scale=2
level=INFO target=arcshot::coordinator app_active surface=frozen
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.tool.line action=Click
level=INFO target=arcshot::coordinator capture_edit action=SelectTool version=0 annotations=0 selection=None tool=Line selected=None style=stroke=#FF3B30 width=3 dash=false
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.tool.arrow action=Click
level=INFO target=arcshot::coordinator capture_edit action=SelectTool version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#FF3B30 width=3 inward=false
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.line.dash action=Click
level=INFO target=arcshot::coordinator capture_edit action=LineOption version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#FF3B30 width=3 dash=true
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.line.width.2 action=Click
level=INFO target=arcshot::coordinator capture_edit action=LineOption version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#FF3B30 width=2 dash=true
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.line.color.green action=Click
level=INFO target=arcshot::coordinator capture_edit action=LineOption version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#34C759 width=2 dash=true
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.arrow.inward action=Click
level=INFO target=arcshot::coordinator capture_edit action=ArrowOption version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#FF3B30 width=3 inward=true
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.arrow.width.5 action=Click
level=INFO target=arcshot::coordinator capture_edit action=ArrowOption version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#FF3B30 width=5 inward=true
level=INFO target=arcshot::ui a11y_action=arcshot.overlay.option.arrow.color.orange action=Click
level=INFO target=arcshot::coordinator capture_edit action=ArrowOption version=0 annotations=0 selection=None tool=Arrow selected=None style=stroke=#FF9500 width=5 inward=true
level=INFO target=arcshot::coordinator preferences_launch completion_sound=true
level=INFO target=arcshot::coordinator completion_sound enabled=false persisted=true
level=INFO target=arcshot::coordinator completion_sound enabled=true persisted=true
level=INFO target=arcshot_macos login_item_status state=Disabled source=menu
```

`style=` 在 LineOption/ArrowOption 行反映被修改工具的样式记忆（`action_style_tool`），
`tool=Arrow` 反映当时激活的工具；两者不矛盾，E2E 断言的是 `style=` 内容。

## 说明与限制

- 与 #30 合并后有两处稳定 ID 空间重叠，已按「保留已集成一侧」处理：
  MAC-E2E-15～17 归 #30 的偏好用例，#26 的四例顺延为 18～21；
  `CORE-SESSION-25` 归 #30 的音效用例，#26 的会话用例改为 `CORE-SESSION-26`。
  #25 的 `CORE-SESSION-24` 与 #30 的 `CORE-SESSION-24` 是合入前既有的重叠，
  不属于本票范围，留给集成者统一整理。
- 焦点竞态是 harness 问题，不是产品行为失败：本机上曾有数轮 E2E 分别卡在
  `press escape --headed`（权限提示、冻结覆盖层、热键会话清理）与录制组合未被前台接收，
  均为 --headed 按键到达前焦点漂移。
  产品侧 #30 已补 `app_active surface=frozen`；本票在脚本里把窗口内组合改为
  `--headed`、对 headed 按键做「最多 4 次、每次重确认前台」的有限重投，
  且只以产品诊断行判成功。补后同一最终脚本连续 5 次（3 Debug + 2 Release）
  21/21 通过。这些补齐前的失败不计 Pass，记录在此。
- 本轮还遇到另一个 worktree（#29）并发启动同名 ArcShot Debug 实例，导致
  `AMBIGUOUS_TARGET` 与全局热键被占用两次 E2E 中途失败；按「串行单实例」原则
  等待其退出后重跑，最终 Debug/Release 各一次在干净窗口内通过。并发实例不是产品失败。
- E2E 会临时写入 Debug/Release 的完成音效偏好并在同一轮还原（日志
  `enabled=false persisted=true` 后再 `enabled=true persisted=true`）；
  真实登录项从未注册/注销（`login_item_status` 全程只读，且无 `login_item_set`）。
- 像素真值由固定文档的 `image/tests/lines.rs` 证明；预览与最终输出共用
  `compose_document`，没有第二条合成路径。`--headed` 坐标点击只作为悬停浮现选项的
  前置，激活证据一律是 AX 行与产品诊断行；没有保存任何截图或取证图像。

## 仍为 Not tested 的项目

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-ANNOTATION-POINTER-26-01 | 真实指针在选区内按下—拖动—松开创建 Line/Arrow | 按 I-06 创建、自动选中、工具保持激活、零长度拒绝 | 未执行：对象绘制只有指针手势，无 AX 节点；`--headed` 坐标拖拽不作验收证据 | Not tested |
| MAC-ANNOTATION-POINTER-26-02 | 真实指针拖动主体、两端点、缩短到零长度重合 | 整体移动/端点改长度方向/既有零长度不删除 | 未执行：端点不是 AX 节点，E2E 无法驱动 | Not tested |
| MAC-ANNOTATION-POINTER-26-03 | 真实指针命中对象、Delete 键删除、Cmd+Z 撤销 | 最上层命中、单步历史、可恢复 | 未执行：选中对象只能经指针命中，删除按钮只在选中后出现 | Not tested |
| MAC-ANNOTATION-VISUAL-26-04 | 在真实屏幕上肉眼比较 2/3/5 点 Solid/Dash 与 Flat/Inward、固定色板 | 两种箭头在每种线宽可区分；虚线间距与箭头比例随线宽缩放 | 未执行：视觉/审美判断属 §4.4，自动化只证明像素真值与几何横截面 | Not tested |
| MAC-ANNOTATION-VISUAL-26-05 | 比较冻结预览与最终 PNG 的标注外观 | 预览与输出一致 | 未执行：需保存/比较取证图像；代码层共用同一合成器，不等于实机观感 | Not tested |
| MAC-ANNOTATION-NATIVE-26-06 | 越界对象、移动/调整选区后的裁剪显示 | 越界不显示、扩大选区重现且标注不缩放 | 未执行：需真实指针与屏幕像素观察，core/image 已覆盖语义 | Not tested |
| MAC-PERMISSION-NATIVE-26-07 | 真实 TCC 授权/撤销、权限提示变体、运行中撤权 | 引导与恢复符合 I-04/I-12 | 未执行：需用户在系统设置授权（§4.4），E2E 只跑已授权状态 | Not tested |
| MAC-START-NATIVE-26-08 | 真实登录项注册/注销、热键冲突恢复、跨注销重启 | 系统实际状态与菜单一致 | 未执行：会改动用户系统状态（#30 亦记为 Not tested） | Not tested |
| MAC-TEXT-IME-26-09 | 中文 IME、拉丁文字、emoji、多行文本提交 | Text 工具行为符合 I-08 | 未执行：Text/IME 属后续切片，本票不实现；与 IME 无关的原生项亦未做实机验收 | Not tested |
| PERF/RESOURCE-26-10 | Release 下 30 样本性能与 24M 导出资源采样 | 达到 T-04 门槛 | 未执行：本票未做性能/资源采样，完整入口只含 Debug+Release 冒烟 | Not tested |
| MAC-E2E-REPEAT-26-11 | Release E2E 重复稳定性统计 | 多次一致通过 | 部分执行：Release 两次 21/21 通过，但未做重复稳定性统计 | Not tested |
