# 端到端（agent-desktop）接入验证记录

本记录证明端到端由代理通过 `agent-desktop` 驱动并已进入脚本入口，不是 MAC-START /
MAC-REGION 等发布矩阵的通过结论。Debug 与 Release 两种 bundle 都跑了同一序列，
全部通过；GPUI 窗口内的交互仍为 **Not tested**（无 accessibility 子树）。
未保存任何截图或屏幕图像，未联网，未改动系统权限设置。

## 实测来源与环境

- 实测实现/脚本 SHA：`d84c1f8405177cc6a810f8f068dc26fc578ec9e5`（会话相位诊断
  提交 + `scripts/e2e-agent-desktop.py`）；其后的文档提交不改产品源码与脚本。
- 时间：2026-11-03T11:40:23Z（Debug）与 2026-11-03T11:40:28Z（Release）。
- 构建配置：Debug 与 Release。Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`；两者都由 `scripts/build-app.sh` 重建。
- 工具：`agent-desktop 1.1.0`（envelope `2.6`），`agent-desktop status` 读出
  `accessibility=granted`、`screen_recording=granted`、`automation=unknown`、
  `platform=macos`。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64。
- macOS：27.2 (26B123)。
- 显示器：内建 ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2。
- 权限：启动 `agent-desktop` 的终端持有辅助功能与屏幕录制；两种 bundle 的屏幕捕获
  均由产品自身路径成功（日志出现真实 `capture_request_us=` 与 `capture_probe_grid`）。
  权限读数只作前置门禁，不作产品通过证据。
- 夹具：`fixture=agent-desktop-e2e-v1`，纯文本与真实桌面；无图像夹具，未使用外部
  应用夹具。热键配置为上一次实机验收保存的 `⌃⌥A`。
- 日志：`target/e2e-agent-desktop-debug.log`、`target/e2e-agent-desktop-release.log`。
  日志为本地构建产物，不入库；本报告保存结果摘要。
- 元数据报告 `worktree_dirty=true` 的原因：采集时只有 `AGENTS.md`、`TEST.md` 与本
  文件的未提交改动（本提交即这些文档），产品源码与脚本与上述 SHA 完全一致。

## 自动记录的环境

Debug（`scripts/record-validation.sh --configuration debug --logs target/e2e-agent-desktop-debug.log,target/e2e-agent-desktop-release.log`）：

```text
commit_sha=d84c1f8405177cc6a810f8f068dc26fc578ec9e5
worktree_dirty=true
timestamp_utc=2026-11-03T11:40:23Z
build_configuration=debug
machine=Mac14,2
architecture=arm64
macos=27.2 (26B123)
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
xcode=Xcode 27.0 (Build version 27A266a)
macos_sdk=27.0 (26A425)
swift=Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)
metal=Apple metal version 32023.921 (metalfe-32023.921.6)
log=target/e2e-agent-desktop-debug.log
log=target/e2e-agent-desktop-release.log
```

Release（`--configuration release`）：

```text
commit_sha=d84c1f8405177cc6a810f8f068dc26fc578ec9e5
worktree_dirty=true
timestamp_utc=2026-11-03T11:40:28Z
build_configuration=release
machine=Mac14,2
architecture=arm64
macos=27.2 (26B123)
rustc=rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo=cargo 1.99.1 (5f94df478 2026-08-27)
xcode=Xcode 27.0 (Build version 27A266a)
macos_sdk=27.0 (26A425)
swift=Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)
metal=Apple metal version 32023.921 (metalfe-32023.921.6)
log=target/e2e-agent-desktop-release.log
```

## 执行

```sh
scripts/e2e-agent-desktop.py            # Debug
scripts/e2e-agent-desktop.py --release  # Release
```

脚本依次门禁 `agent-desktop status` 与在跑实例、重建 bundle、核对
`CFBundleIdentifier`/`CFBundleName`、启动 bundle 可执行文件并把 stderr 写入
`target/e2e-agent-desktop-<configuration>.log`，再逐条断言并打印
`mac_e2e_XX=pass …`；任一断言失败即非零退出。两次 Debug、一次 Release 连续运行
结果一致。

## 稳定子用例

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-E2E-09 | 启动后 `list-apps --app "ArcShot"` | 只出现被测 bundle | Debug：`dev.arcshot.app.debug` / `ArcShot Debug`；Release：`dev.arcshot.app` / `ArcShot`；均恰好一个实例 | Pass |
| MAC-E2E-01 | `snapshot --surface menu-bar-extras --app <name>` | 一个状态项（role `menuitem`、name `ArcShot`）与四个菜单项，`native_id` 为 selector | `ref_count=5`；四项依次 `startCapture:`、`hotkeySettings:`、`screenRecordingPermissions:`、`terminate:`；不需要前台 | Pass |
| MAC-E2E-02 | 驻留时 `list-windows --app <name>` | 无可捕获自有窗口 | `data=[]` | Pass |
| MAC-E2E-03 | headless `click` 开始截图 | 出现 capture 请求、冻结窗口提交与 `to=Frozen` 相位 | `capture_request_us=`、`capture_probe_grid`、`capture_phase … to=Frozen action=Completed`、`capture_wake_window_submit_us=` 各 1 条；`list-windows` 仍为 0 | Pass |
| MAC-E2E-05a | `press escape --headed` | 按键投递成功，无残留可捕获窗口 | `press` 返回 `ok`/`delivered_unverified`；`list-windows --app` 为空 | Pass |
| MAC-E2E-05b | 读 Esc 之后的 `capture_phase` 行 | 会话离开冻结态且动作为取消 | `from=Frozen to=Resident action=CancelAttempt` | Pass |
| MAC-E2E-04 | 先按未注册组合，再按日志 `hotkey_launch config=` 的 `⌃⌥A` | 前者不产生会话，后者恰好产生一次 | 未注册组合前后 `capture_request_us` 计数不变；`⌃⌥A` 后计数 +1 并进入冻结；不需要前台 | Pass |
| MAC-E2E-07 | 点击 屏幕录制权限… 后 Esc | 出现提示窗口，Esc 后消失 | 提示窗口 `560×282`、`is_focused=true`；Esc 后 `list-windows` 为空 | Pass |
| MAC-E2E-06 | 点击 截图快捷键… | `menu_hotkey_settings` 与一个可寻址窗口 | 日志 1 条；窗口 `480×362`、标题 `<name>` | Pass |

两次实测中 GPUI 窗口出现过比设定值宽 2 点的瞬时读数（`562`/`482`），脚本按 ±4 点
容差等待稳定尺寸；该容差只吸收布局瞬时态，不放宽窗口应存在且数量正确的断言。

## 实测到的可观察边界

- 会话相位现在是唯一可信的相位真相：开始、取消、重启与失败各有一行
  `capture_phase from=… to=… action=…`，不打印帧内容（`PRIVACY-01` 语义不变）。
- GPUI 0.2.2 不上报 accessibility 子树（窗口 `AXChildren` 为 0），`snapshot --app`
  与 `list-surfaces --app` 为空；状态项与 NSMenu 是唯一可寻址的交互面。
- 无标题栏的冻结覆盖层不出现在 `list-windows`，但确实在屏
  （`CGWindowListCopyWindowInfo`：`layer=101`，1470×956，自有 pid）。因此本记录只
  能给出“会话离开冻结态”的证据，不能给出 NSWindow 销毁级证据。
- 本机全局窗口清单不稳定（`global_window_owner_churn`，ArcShot 未运行时同样），
  不带 `--app` 的 `list-windows` 与 `snapshot --window-id` 都不可用；
  `list-windows --app` 稳定。
- `press escape --app "<name>"` 在无焦点 AX 元素时返回 `ACTION_FAILED`
  （`no verified focused element`）；不带 `--app` 的 `--headed` 投递可用，且只在
  产品已 `activate` 自己的窗口/提示后发出。
- `press` 注入的组合是真实 CGEvent 投递（`mechanism=physical_synthetic`），本轮
  命中已注册热键；#21 记录中“手工合成 CGEvent 触发不到热键”只适用于当时那两次手工
  投递，不适用于本工具。

## 未覆盖与限制

- 冻结覆盖层 NSWindow 销毁、GPUI 窗口内的按钮/输入区/冻结画面/标注工具：无探针、
  无 AX 节点，仍为 **Not tested**；不得用 `--headed` 坐标点击或截图比较补足。
- 真实 TCC 提示、设置返回重检、运行中撤权、热键录入与保存、IME 候选窗：属用户
  授权或无可寻址节点，未覆盖。
- Release 只跑了同一序列一次；未做重复稳定性统计、性能与资源测量。

## #30 追加：状态菜单两个偏好项与跨重启（2026-11-04）

以上各节记录基座迁移轮（0.2.2 → 0.3.11）的实测，**不覆盖**后续票据新增的可寻址
产品面。本节只记录 #30 在合并 #25 之后的同一入口上跑出的结果；完整环境、命令与
子用例见 [`30-sound-login.md`](30-sound-login.md)。

- 实测实现/脚本 SHA：`efa3b7a395080733a902d76bf583454ddaa248b3`。
- 时间：2026-11-04T06:07:07Z，Debug 配置，`agent-desktop 1.1.0`（envelope `2.6`）。
- 入口：`scripts/e2e-agent-desktop.py`，17 个用例全部 `pass`，结尾
  `mac_e2e=pass configuration=debug … cases=17`。
- 状态菜单现在有六个菜单项：四个既有命令 + `toggleCompletionSound:` +
  `toggleLaunchAtLogin:`，`native_id` 即 selector；MAC-E2E-01 已同步更新。
- 新增：MAC-E2E-15（AX 切换完成音效并写偏好）、MAC-E2E-16（两次真实重启验证偏好
  跨重启并还原）、MAC-E2E-17（菜单打开读取真实 `SMAppService` 状态，全程没有
  `login_item_set`）。真实登录项注册/注销仍为 **Not tested**。
- 焦点前置：冻结覆盖层打开后产品写一行 `app_active surface=frozen`，E2E 等它再发
  `--headed` Esc；带标题窗口改用 `snapshot --surface focused` 确认前台。补此接缝前
  同一代码上有两轮失败（`--headed` Esc 与录制组合未被前台接收），补后连续 17/17。
- 日志：`target/e2e-agent-desktop-debug.log` 与
  `target/e2e-agent-desktop-debug-restart-1/-2.log`（每次重启单独一份）。
