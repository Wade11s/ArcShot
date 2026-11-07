# #29 Standard / Compact 选项密度与偏好记忆验证记录

本记录覆盖 #29 落地的 I-09/I-04 选项密度：状态菜单的 **Standard / Compact**（默认
Compact）、本地偏好域跨会话与重启记忆、两种密度共享同一套选项、切换密度不改变工具、
每工具样式、选区、对象与进行中的编辑，以及 `arcshot.overlay.density` 的 AX 可观察状态。
证据来自 core 权威会话状态、app 注入式偏好流、原生 `NSUserDefaults` 往返、完整
`scripts/check-all.sh` 与由代理用 `agent-desktop` 驱动的 22 条 E2E。图标像素观感、
悬停收起延迟、键盘单独访问选项与新版密度在极小选区下的观感保持 **Not tested**。
本票没有保存用户截图、没有联网、没有请求新权限；E2E 临时改动的完成音效与选项密度
偏好都在同一轮还原。

## 实测来源与环境

- 基线：`spec/v1` = `5c909e5`（含 #22/#23/#24/#25/#26/#30）；#29 功能提交
  `29f5cb9`，与 `spec/v1` 的合并 `cedfa08`，收尾 `9a5befa`。本记录落地后再增加一个
  仅含文档与 `TEST.md` 的提交。
- 实测实现/脚本 SHA：`9a5befa9e736ac68c9669705e8ac8272deca4d6c`
- 时间：2026-11-04T07:44:15Z（`scripts/check-all.sh` 完成）与
  2026-11-04T07:45:28Z（Debug E2E 完成）。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；均由 `scripts/build-app.sh` 重建，
  本机自签名身份 `ArcShot Dev`。E2E 本轮只跑 Debug 配置。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)，`/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.0.sdk`。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，Target arm64-apple-macosx27.2.0。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：内建屏 ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；产品日志
  `capture_request_us=150290 display=1 pixels=2940x1912 stride=11760 scale=2` 给出物理
  2940×1912。未保存屏幕图像。
- 权限：`agent-desktop status` 为 `accessibility=granted`、
  `screen_recording=granted`、`automation=unknown`；被测 bundle 的 Screen Recording
  preflight 通过。未改动 TCC，未请求通知权限，未注册/注销登录项；只读状态来自
  `login_item_status state=Disabled source=menu source=launch read_only=true`。
- 夹具：`fixture=density-e2e-v1`：真实 Debug/Release bundle、core 固定文档快照与
  app 注入偏好结果、`agent-desktop 1.1.0`（envelope `2.6`）、真实 `NSUserDefaults`
  偏好域（两轮往返并还原）、真实 `SMAppService` 只读状态。无图像夹具、无外部应用夹具。
- 日志：`/tmp/checkall-29.log`、`/tmp/e2e-29.log`、
  `target/e2e-agent-desktop-debug.log`、`target/e2e-agent-desktop-debug-restart-1.log` …
  `-restart-4.log`、`target/29-*` 构建日志。日志只含文本/数值/状态，不含像素。

## 命令与实际输出

```text
scripts/check-all.sh                     EXIT=0
  cargo fmt --all -- --check             pass
  cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   pass
  cargo test --locked --workspace --all-features                                 168 passed, 0 failed
  cargo doc --locked --workspace --no-deps                                       pass
  validation_reports=pass directory=docs/validation
  capture_menu_smoke=pass queued_commands=6 menu_items=7 no_native_capture=true
  menu_items_smoke=pass count=7 density_index=3
  preference_items_smoke=pass density_selector=toggleDensity: sound_selector=toggleCompletionSound: login_selector=toggleLaunchAtLogin: state_from_system=true
  density_smoke=pass key=OptionDensity roundtrip=true restored=true default=Compact
  privacy_smoke=pass resident_pid=46404 network_sockets=0 lsof_status=1
  startup_smoke=pass activation_policy=Accessory quit_action=terminate:   (Debug + Release)

scripts/e2e-agent-desktop.py             EXIT=0
  agent_desktop=agent-desktop 1.1.0
  agent_desktop_status permissions={"accessibility": {"state": "granted"}, "automation": {"state": "unknown"}, "screen_recording": {"state": "granted"}}
  mac_e2e_01=pass menu_items=7 refs=8
  mac_e2e_22=pass density=Compact->Standard->Compact option_set=reachable tool_and_style=preserved restart=restored ax=dock_value
  mac_e2e=pass configuration=debug log=target/e2e-agent-desktop-debug.log cases=22
```

`defaults read` 交叉核对：E2E 结束后用户偏好域的 `OptionDensity` 与 `CompletionSoundEnabled`
均等于启动前的值；`login_item_set` 在整段日志中从未出现。

## 选定布局数值（实现选择，不是规格承诺）

| 项目 | Compact | Standard |
| --- | --- | --- |
| 工具坞按钮边长 | 28 pt | 36 pt |
| 工具坞间距 | 4 pt | 8 pt |
| 选项条图标边长 | 20 pt | 26 pt |
| 分组标题 | 精简/隐藏 | 显示分组说明 |
| 完成 / 保存入口 | 不缩小 | 不缩小 |

I-09 只要求两种密度提供同一套选项、不清除工具/样式/选区/对象/文字，并要求工具坞与
完成/保存入口不因此缩小到不可用；旧浏览器原型的精确像素、收起延迟不是规格。上表是
本票为本机 1470×956 逻辑点所选的数值，尚未有人在极小选区或近全屏下做视觉判断。

## 稳定用例

| ID | 前置 | 步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- | --- |
| CORE-DENSITY-01 | 空偏好存储 | 新建会话、切换密度、重新开始会话 | 默认 Compact；切换只改会话状态；新会话恢复真实存储值，不重置为默认 | 断言通过 | Pass |
| CORE-DENSITY-02 | 冻结文档、Rectangle + 蓝色 5 点样式、有效选区与对象 | 切换 Standard / Compact | 工具、每工具样式、选区、对象、文档版本与编辑状态完全不变；密度不是文档编辑，不产生历史步骤 | 断言通过 | Pass |
| APP-EFFECT-33 | 注入偏好流 | 启动、切换、重启 | 默认 Compact；每次切换持久化；重启读取真实值 | 断言通过 | Pass |
| APP-EFFECT-34 | 注入偏好流 | 打开菜单并切换 | 意图来自新的实际读取，不来自上次意图；成功写入才呈现已生效 | 断言通过 | Pass |
| APP-EFFECT-35 | 偏好读取失败 | 切换密度 | 报告不可读，不静默变更意图，不当作成功 | 断言通过 | Pass |
| APP-EFFECT-36 | 偏好写入失败 | 切换密度 | 重读实际状态，显示失败，不把意图显示为已生效，保留可重试 | 断言通过 | Pass |
| APP-EFFECT-37 | 一次切换 | 观察偏好副作用 | 只产生一次切换相关副作用，不级联音效/登录项动作 | 断言通过 | Pass |
| MAC-START-29-01 | 正式 bundle `--smoke-test` | 枚举菜单 action、selector、真实状态与偏好往返 | 七个 action、`toggleDensity:`、`state_from_system=true`、`density_smoke=pass … default=Compact` | Debug 与 Release 均通过 | Pass |
| MAC-START-29-02 | 真实偏好 suite 域 | 缺失/写入/新实例读取/删除 | 缺失为 Compact，往返一致，删除回默认 | 通过 | Pass |
| MAC-E2E-01 | 正式 Debug bundle 启动 | `snapshot --surface menu-bar-extras --app` | 七个菜单项，`native_id` 与 selector 一致 | `mac_e2e_01=pass menu_items=7 refs=8` | Pass |
| MAC-E2E-22 | 冻结会话、Rectangle + 一个已改样式、已知初始密度 | 状态菜单切换两次、读工具坞 AX 值、确认选项仍可寻址、重启一次、还原 | `option_density density=… persisted=true`；`arcshot.overlay.density` 值随密度变化；工具/样式不变；重启后仍是切换值；结束时还原用户原值 | `mac_e2e_22=pass density=Compact->Standard->Compact option_set=reachable tool_and_style=preserved restart=restored ax=dock_value` | Pass |
| MAC-E2E-02…21 | 见 `TEST.md` §4.3 | 全量 E2E 回归 | 22 条全部通过，含 #26 的四个工具与 #30 的两个偏好 | `mac_e2e=pass … cases=22` | Pass |

## Not tested（附原因）

- 两种密度的图标/间距/分组标题观感、贴边与小选区布局、悬停浮层的收起延迟：属于
  `TEST.md` §4.4 的视觉/审美判断，本记录只给出所选数值，不宣称观感通过。
- 键盘单独访问悬停选项：选项条仍需指针悬停后才进入 AX 树，与 #25 记录的既有缺口相同。
- 密度偏好在崩溃/强制退出与写入之间的时序、`NSUserDefaults` 跨进程同步：未注入原生失败。
- Release 配置的 E2E：本轮只跑 Debug；Release 原生启动冒烟在 `check-all.sh` 中通过。
- 登录项真实注册/注销与失败提示窗口：会改动用户系统登录项并可能需要系统设置批准，
  E2E 与冒烟都不执行；状态机由 APP-EFFECT-28～32 覆盖。
- Text/IME、Blur、滚动截图、性能与资源门槛：属于后续票据，本记录不覆盖。

## 合入说明

`spec/v1` 在 #29 实现期间移动到 `5c909e5`（#26 Line/Arrow）。合并冲突集中在导入、
`ui.rs` 的 `option_strip`/`DockMetrics`、E2E 的 `press_until` 与用例计数：保留 #26 的
四个工具、`press_until` 与 MAC-E2E-18…21，本票的密度 E2E 用例改编号为 **MAC-E2E-22**，
总数 22。`cedfa08` 与 `9a5befa` 已重新跑过 `scripts/check-all.sh`（见上）。
