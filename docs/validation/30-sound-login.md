# #30 完成音效偏好与“登录时启动”验证记录

本记录覆盖 #30 落地的两条 I-04 菜单偏好：**完成音效**（默认开、本地跨重启记忆、
关闭时成功输出静默而失败照常报错）与**登录时启动**（真实 `SMAppService` 登录项、
菜单打开读取实际状态、切换失败重读并报错）。自动化在 core 语义、app 注入式策略、
原生偏好域/`SMAppService` 只读与 Debug E2E 上执行；真实登录项注册/注销（会改动
用户系统登录项）未执行，保持 **Not tested**。本票没有捕获屏幕、保存取证图像、
修改剪贴板、请求通知权限或联网；仅 `defaults` 偏好域被脚本写入后还原。

## 实测来源与环境

- 基线：`spec/v1` = `52790c2`（含 #25 Rectangle 工具切片）；本票提交从 `9ba5641`
  起算，合并后实测。
- 实测实现/脚本 SHA：`efa3b7a395080733a902d76bf583454ddaa248b3`（本票功能 +
  `scripts/check-startup.sh` + `scripts/e2e-agent-desktop.py` + `TEST.md`，含合并
  #25 后的焦点前置修正）；后续提交只增加本证据文档。
- 时间：2026-11-04T06:07:07Z（Debug E2E 本轮完成）与 2026-11-04T06:09:10Z
  （`scripts/check-all.sh` 完成）。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；均由 `scripts/build-app.sh` 重建，
  本机自签名身份 `ArcShot Dev`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift version 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal version 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：preflight ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；
  本轮未捕获屏幕内容。
- 权限：证据采集时 Debug bundle 的 Screen Recording preflight=true、guide_seen=true；
  `agent-desktop status` 为 `accessibility=granted`、`screen_recording=granted`、
  `automation=unknown`、`platform=macos`。未改动 AC 权限，未请求通知权限。
  登录项状态由 `SMAppService` 只读得到 `Disabled`（本机原本没有 ArcShot 登录项）。
- 夹具：`fixture=preferences-e2e-v1`：真实 Debug bundle、真实 NSUserDefaults 偏好
  域、真实 `SMAppService.mainAppService` 状态、合成 2×2 PNG（仅既有保存冒烟复用）。
  完成音效偏好在 E2E 中被切换两次并还原；未修改用户其他偏好。
- 日志：`target/30-check-all-debug-release.log`、`target/e2e-agent-desktop-debug.log`、
  `target/e2e-agent-desktop-debug-restart-1.log`、`target/e2e-agent-desktop-debug-restart-2.log`、
  `target/startup-smoke-debug.log`、`target/startup-smoke-release.log`。日志是本地
  构建产物，不入库；本报告保存摘要。
- `worktree_dirty=false`（实测提交时）；本报告本身是之后的纯证据文档提交。

## 执行

```sh
scripts/check-all.sh > target/30-check-all-debug-release.log 2>&1
cargo test --locked -p arcshot-core --test output
cargo test --locked -p arcshot-app --test preferences
cargo test --locked -p arcshot-macos --lib
scripts/check-startup.sh
scripts/check-startup.sh --release
scripts/e2e-agent-desktop.py
defaults read dev.arcshot.app.debug CompletionSoundEnabled
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、doc、全部 Rust 测试（含本票
CORE-SESSION-24～25、APP-EFFECT-27～32、MAC-START-30-02）、验证工具/依赖/架构/
真实文字后端/无网络、Debug 与 Release 原生驻留与退出冒烟。

## 稳定自动化子用例

前置为上述 SHA、锁定依赖与合成夹具；原生冒烟只读取登录项状态。所有 “实际”
列为该次运行的观察结果或脚本断言的行为。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-SESSION-24 | 冻结并建立选区；`set_completion_sound(false)` 后完成复制 | 默认开；关闭后成功只发出 `DismissOverlay`，会话回到 Resident，没有成功文字 | `test core_session_24_completion_sound_preference_gates_only_the_success_sound ... ok` | Pass |
| CORE-SESSION-25 | 关闭音效后复制失败；新会话再成功；重新开启后再次成功 | 失败仍 `ReportOutputFailure`；偏好不被新会话重置；重新开启后恢复 `PlayCompletionSound` | `test core_session_25_sound_off_still_reports_failures_and_survives_a_new_session ... ok` | Pass |
| APP-EFFECT-27 | 注入持久值 true 构造策略，切换两次；再用 false 构造 | 默认开；每次切换返回 `StoreCompletionSound(<新值>)`；重启构造保留注入值 | 6 项 `arcshot-app` 偏好测试全部 ok | Pass |
| APP-EFFECT-28 | 已知状态 Disabled/Enabled，点击 登录时启动 | 意图来自新的实际读取，不用上次值；实际 Enabled 时请求关闭 | `SetLoginItem(true)` / `SetLoginItem(false)` 与观察一致 | Pass |
| APP-EFFECT-29 | `SetLoginItem` 返回 Applied 后读取实际 | 以读取验证结果；不匹配时报告失败、状态显示实际值 | 匹配时无效果；仍 Disabled 时 `Notice(LoginItemToggleFailed)` | Pass |
| APP-EFFECT-30 | 切换请求被拒绝 | 先报告失败再重读实际状态；提示可关闭，菜单可重试 | `[Notice(ToggleFailed), ReadLoginItemStatus]`；Dismiss 后重试发出新请求 | Pass |
| APP-EFFECT-31 | 切换前读取失败；Applied 后验证读取失败；启动读取失败 | 前两者不盲改并报错；启动读取失败不弹窗 | `Notice(LoginItemUnreadable)`；启动路径无通知 | Pass |
| APP-EFFECT-32 | Applied 后实际状态为 RequiresApproval | 报告需要批准，不显示为已启用 | `Notice(LoginItemApprovalRequired)`，状态保持 RequiresApproval | Pass |
| MAC-START-30-01 | Debug/Release `--smoke-test` | 五个菜单 action 各自入队；两个偏好项 selector/状态与真实偏好与 `SMAppService` 状态一致 | `capture_menu_smoke=pass queued_commands=5`、`preference_items_smoke=pass sound_selector=toggleCompletionSound: login_selector=toggleLaunchAtLogin: state_from_system=true`（两种配置各一次） | Pass |
| MAC-START-30-02 | `cargo test -p arcshot-macos --lib` | 真实偏好 suite 域：缺失=开；写入 false 后新实例读取 false；删除后回默认 | `test preferences::tests::mac_start_30_02_completion_sound_round_trips_in_a_real_preferences_domain ... ok` | Pass |
| MAC-START-30-03 | 同一冒烟进程读取 `SMAppService.mainAppService.status` | 真实状态被读出，且不注册/不注销 | `login_item_status_smoke=pass state=Disabled read_only=true`（两种配置各一次） | Pass |
| MAC-START-30-04 | 同一冒烟进程对产品偏好域写入/读回/还原完成音效 | 真实偏好域往返成功，最终值还原 | `preference_smoke=pass key=CompletionSoundEnabled roundtrip=true restored=true`（两种配置各一次） | Pass |
| PRIVACY-01 | `scripts/check-no-network.sh` | 驻留 ≥0.5 秒且无网络/监听 socket | `privacy_smoke=pass resident_pid=… network_sockets=0 lsof_status=1` | Pass |
| DEPENDENCY-01 / ARCH-01 | 完整入口 | 新直接依赖来源为 crates.io；四 crate 方向不变 | `dependency_inspection=pass`、`dependency_sources=path,registry+https://github.com/rust-lang/crates.io-index`、`architecture check passed` | Pass |

## E2E（Debug，17 个用例全部通过）

命令：`scripts/e2e-agent-desktop.py`（`agent-desktop 1.1.0`，envelope `2.6`）。
本轮把所有既有用例与新增用例跑在同一序列；新增项：

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-01（更新） | 启动 Debug bundle 后 `snapshot --surface menu-bar-extras` | 状态项菜单六项，`native_id` 为 selector | `mac_e2e_01=pass menu_items=6 refs=7`；依次 `startCapture:`、`hotkeySettings:`、`screenRecordingPermissions:`、`toggleCompletionSound:`、`toggleLaunchAtLogin:`、`terminate:` | Pass |
| MAC-E2E-05a/05b（前置更新） | 冻结后等产品报告 `app_active surface=frozen`，再 `press escape --headed` | 只有真正前台后才投递；Esc 后相位回 Resident | 日志出现 `capture_wake_window_submit_us=492281` 后 `app_active surface=frozen`，随后 `from=Frozen to=Resident action=CancelAttempt` | Pass |
| MAC-E2E-15 | 读 `preferences_launch completion_sound=true`；headless 点击 完成音效 两次 | 每次写 `completion_sound enabled=… persisted=true`，值先翻转再还原 | `mac_e2e_15=pass sound=true->false->true selector=toggleCompletionSound:`；日志出现 `completion_sound enabled=false persisted=true` 与 `… enabled=true persisted=true` | Pass |
| MAC-E2E-16 | 切到 false → 菜单退出 → 重启 → 读取 → 切回 true → 退出 → 重启 → 读取 | 两次重启分别恢复 false 与 true；结束时用户偏好还原；始终单实例 | `mac_e2e_16=pass restart_restored=false restart_restored=true preference=local`；重启日志首行分别为 `preferences_launch completion_sound=false`、`…=true` | Pass |
| MAC-E2E-17 | MAC-E2E-01 打开状态菜单后读日志 | 出现 `login_item_status … source=menu`，与 `source=launch` 一致；全程没有 `login_item_set` | `mac_e2e_17=pass login_item_actual=Disabled source=menu source=launch read_only=true` | Pass |
| MAC-E2E-02～12、13～14 | 既有驻留/捕获/取消/权限提示/热键/覆盖层与 #25 工具坞用例；录制组合前要求 `snapshot --surface focused` 的 `app` 是被测显示名 | 原行为不变 | `mac_e2e_02…05b`、`07`、`10`、`06`、`11`、`13`、`14`、`12` 全部 `pass`；结尾 `mac_e2e=pass configuration=debug … cases=17` | Pass |

被引用的产品 stderr 原文（同一轮，来自 `target/e2e-agent-desktop-debug*.log`）：

```text
level=INFO target=arcshot::coordinator capture_wake_window_submit_us=492281 pixels=[2940, 1912]
level=INFO target=arcshot::coordinator app_active surface=frozen
level=INFO target=arcshot::coordinator preferences_launch completion_sound=true
level=INFO target=arcshot_macos login_item_status state=Disabled source=menu
level=INFO target=arcshot::coordinator completion_sound enabled=false persisted=true
level=INFO target=arcshot::coordinator completion_sound enabled=true persisted=true
level=INFO target=arcshot::coordinator login_item_status state=Disabled source=launch
# restart-1:
level=INFO target=arcshot::coordinator preferences_launch completion_sound=false
level=INFO target=arcshot::coordinator completion_sound enabled=true persisted=true
# restart-2:
level=INFO target=arcshot::coordinator preferences_launch completion_sound=true
```

`defaults read dev.arcshot.app.debug CompletionSoundEnabled` 在整轮结束后为 `1`，
与运行前的用户取值一致；Release 冒烟同样写回并还原了 `dev.arcshot.app` 域的取值。

## 说明与限制

- “完成音效关闭”只证明 core 不再发出 `PlayCompletionSound` 效果：没有成功文字或
  通知，失败仍走 `ReportOutputFailure`。真实静音与音量属于实机观感，未测量。
- 焦点前置是实跑中补出来的：同一份代码上曾有两轮 E2E 分别卡在 `press escape
  --headed` 与录制组合没有被设置窗口接收，日志停在该会话的 `Frozen`/等待候选。
  根因是 `--headed` 按键与 headless 录制键依赖前台焦点，而脚本只按日志顺序投递；
  产品侧因此补了 `app_active surface=frozen` 一次性诊断行，脚本在覆盖层用它、
  在带标题窗口用 `snapshot --surface focused` 挡在按键之前。补后同一提交连续两次
  17/17 通过；这两次失败不是产品行为失败（覆盖层取消另有 MAC-E2E-12 的 AX 证据），
  但也不能算 Pass，故记录在此。
- 菜单打开时 `validateMenuItem:` 调用 `SMAppService` 只读状态；这解决了“显示意图
  而非实际值”，但 `SMAppService` 的读取本身没有错误返回值，生产路径总是得到
  Enabled/Disabled/RequiresApproval 之一。`None`（不可读）分支由 APP-EFFECT-31
  的注入签名覆盖，产品适配器目前不会产生它。
- 完成音效偏好写在 bundle 自己的 `NSUserDefaults` 域，`store_completion_sound`
  在写入后调用 `synchronize()`，让“点击后立刻退出”也能在下次启动恢复；该时序由
  MAC-E2E-16 的真实重启证明，未覆盖强制断电/崩溃写入顺序。
- 本轮 E2E 会临时写入 Debug 偏好域并在同一轮还原；脚本中途失败时可能留下完成音效
  为关闭，需要手工恢复（`defaults write dev.arcshot.app.debug CompletionSoundEnabled -bool true`
  或再点一次菜单）。本轮未发生。
- 原生冒烟只在真实菜单里发送 preference action 到 `CommandTarget`：冒烟进程没有
  coordinator，因此只验证 selector→命令入队，不触发注册/注销或偏好写入。
- 本次 `check-all.sh` 的 Debug/Release 冒烟都改了产品偏好域并还原，属于测试副作用，
  已在报告中记录；没有新增网络、通知、TCC 或遥测依赖。

## 仍为实机/受控的原生子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-START-MANUAL-30-01 | 打开状态菜单 → 点 登录时启动 打开 | 系统把当前 bundle 注册为登录项；`SMAppService` 状态变为 Enabled（或 RequiresApproval 并提示批准）；下次登录启动 ArcShot；再次关闭后注销 | 未执行：会改动用户系统登录项并可能需要系统设置批准；E2E/冒烟只读 | Not tested |
| MAC-START-MANUAL-30-02 | 在系统设置中拒绝/撤销登录项后回到菜单 | 菜单显示实际状态（RequiresApproval 按未启用显示），按需提示批准，不显示为已启用 | 未执行：需要系统设置交互与批准 | Not tested |
| MAC-START-MANUAL-30-03 | 制造 `SMAppService` 拒绝（如未签名 bundle 的 register） | 应用内提示窗口出现、可关闭、可从菜单重试；菜单状态保持系统实际值 | 未执行：需要真实拒绝与用户可见窗口；失败分支由 APP-EFFECT-29～32 覆盖 | Not tested |
| MAC-START-MANUAL-30-04 | 退出登录后重新登录 | 已注册的登录项在下次登录启动 ArcShot；未注册时不启动 | 未执行：需要注销/重启与会话级验证 | Not tested |
| MAC-RECOVERY-MANUAL-30-05 | 关闭完成音效后完成一次真实复制 | 无声、无成功文字/通知；失败仍提示 | 未执行：静音是可听观感项，自动化只证明不发声音效效果 | Not tested |
| E2E-RELEASE-30 | `scripts/e2e-agent-desktop.py --release` | Release bundle 同样通过 17 个用例 | 未执行：本轮 E2E 只跑 Debug；Release 原生冒烟通过 | Not tested |
