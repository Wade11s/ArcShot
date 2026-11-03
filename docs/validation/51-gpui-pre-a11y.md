# #51 `gpui-pre` 0.3.11 可访问性可行性验证

本记录只证明 #51 的基座可行性与观察能力，**不**是产品迁移结果，也不是 V1 发布验收。产品依赖与产品代码在合并分支上未改动；全部试验都在丢弃分支 `ticket/51-spike-gpui-pre` 上完成。

## 来源与环境

- 结合点：`spec/gpui-a11y` `0548295`（本记录所在票据分支 `ticket/51-gpui-a11y-base` 的基点）。
- 实测实现/脚本 SHA：`8a28d545ca1620698742dbc1968c87dfdc4bd8e2`（丢弃分支 `ticket/51-spike-gpui-pre` 的 0.3.11 版本，worktree `.worktrees/51-spike-gpui-pre`，**不合并**）；同一分支的 `817eada` 是 0.3.8 版本。
- 实验依赖：`gpui-pre 0.3.11` + `gpui-pre-platform 0.3.11`（含 `gpui-pre-macos/-apple 0.3.11`、`accesskit 0.24.1`、`accesskit_macos 0.26.3`、`accesskit_consumer 0.38.0`），均 `=0.3.11`；`Cargo.lock` 已更新但只存在于丢弃分支。
- 时间：2026-11-03T12:13:01Z 起，2026-11-03T12:37:30Z 止（UTC；含 0.3.8 与 0.3.11 两轮）。
- 机器：MacBook Air（Apple Silicon，arm64），macOS 27.2 (26B123)。
- Rust/Cargo：1.99.1（rustc `58cf8f9c35b355199727627d07ae34a4e2846d67`，cargo `5f94df478 2026-08-27`）。
- Xcode：27.0 (27A266a)；SDK 27.0 (26A425)。
- Swift：6.4 (`swiftlang-6.4.0.34.1`)；Metal/AIR-LLD：32023.921。
- 构建配置：Debug。构建目录 `CARGO_TARGET_DIR=/tmp/arcshot-51-target`；Release 未构建。
- 显示器：内建屏 1470×956 逻辑点，`scale=2`（产品诊断 `capture_request_us=… pixels=2940x1912 stride=11760 scale=2`）。
- 权限：`agent-desktop status` 报告 Accessibility 与 Screen Recording 均 `granted`；本次没有让用户做新的 TCC 授权。
- 夹具：无外部图像或网络夹具；截图输入是内建屏实时抓帧（`display=1`），未保存任何取证图像。
- 工具：`agent-desktop 1.1.0`（envelope 2.6，macOS）。
- 日志：丢弃分支 `target/startup-smoke-debug.log` 与产品 stderr `/tmp/arcshot-51-app.log`；完整逐条命令输出在 `/tmp/arcshot-spec-notes/51-proof.log`（外部工作记录，不随仓库提交）；摘要见 ADR-0001 附录 A。本记录只保留结论与关键行。

## 版本矩阵

| 快照 | 发布 | 结果 | 首个阻塞点 / 备注 |
| --- | --- | --- | --- |
| **0.3.11** | 2026-11-02 | **Pass（选用）** | 用 0.3.8 适配后的代码零改动编译；文字后端、AX 取证、`check-startup.sh`、`check-no-network.sh`、`inspect-dependencies.py` 全过；仅 `check-architecture.sh` 因包名断言失败（#52 修）。 |
| 0.3.10 | 2026-10-26 | Not tested | 最新已通过，未触发向下回退。 |
| 0.3.9 | 2026-10-12 | Not tested | 同上。 |
| 0.3.8 | 2026-10-05 | Pass（后备） | 第一轮在此版本通过同样取证；但比最新旧，且拉进遗留 `cocoa 0.26 → block 0.1.6`、596 个解析包、43 个重复包名。 |

## 稳定用例

| ID | 前置与步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| SPIKE-BUILD-01 | 丢弃分支替换基座为 `gpui-pre`/`gpui-pre-platform` 后 `cargo build -p arcshot-app` | 能编译 | 0.2.2 → 0.3.8 有 12 个编译错误（Timer / Application::new / update Result / Window::focus / DisplayId），逐条修复后通过；0.3.8 → 0.3.11 零错误，`Finished dev profile … 23.38s` | Pass |
| SPIKE-TEXT-01 | 直接运行 `/tmp/arcshot-51-target/debug/arcshot --check-text-backend` | 真实 macOS 文字后端 | `text_backend=pass font=Helvetica i_advance=3.5546875px W_advance=15.1015625px`；未运行 `scripts/check-text-backend.sh` 包装脚本 | Pass |
| SPIKE-RUN-01 | 启动二进制，`agent-desktop snapshot --surface menu-bar-extras --app arcshot` | 驻留并可驱动菜单 | 读到状态项与四个菜单项（selector `startCapture:`/`hotkeySettings:`/`screenRecordingPermissions:`/`terminate:`）；`list-apps` 报 `presentation: background`（无 Dock） | Pass |
| SPIKE-A11Y-01 | 点击“截图快捷键…”打开 GPUI 窗口，`snapshot --app arcshot` | 至少一个可寻址节点 | `dialog "截图快捷键"` 下 `button "录入新快捷键"`（`ref_id`）与 `button "关闭"`（`AXIdentifier=关闭`） | Pass |
| SPIKE-A11Y-02 | 对该窗口 `click @ref`（headless `AXPress`） | 引用可驱动产品动作 | `list-windows --app` 变空；产品 stderr `a11y_click label=关闭 command=HotkeySettingsClosed(…)` 与 `hotkey_dispatch action=SettingsClosed` | Pass |
| SPIKE-A11Y-03 | 从菜单开始截图（`WindowKind::PopUp` + `titlebar: None`），隔 ~1.5s 重读 `snapshot --app arcshot` | 无标题覆盖层同样上报 | `group "ArcShot 截图覆盖层"`（`AXIdentifier=capture-overlay`）+ `statictext capture-status` + `button capture-cancel`（`ref_id`）；`list-windows` 仍为空 | Pass |
| SPIKE-A11Y-04 | 对覆盖层按钮 `click @ref` | 引用可驱动产品动作 | 产品 stderr `capture_phase from=Frozen to=Resident action=CancelAttempt` | Pass |
| SPIKE-A11Y-05 | 0.2.2 对照：集成 tip 的 Debug 二进制打开同一快捷键窗口后 `snapshot --app arcshot` | 无产品节点 | 只有 AppKit 的 Close/Zoom/Minimize，无产品节点；该二进制构建于 2026-11-03T11:35Z，未逐位核对 commit SHA | Pass（对照） |
| SPIKE-GATE-01 | 丢弃分支 `scripts/check-startup.sh` | 启动/退出冒烟 | `bundle_identity_smoke=pass`、`capture_window_exclusion_smoke=pass`、`capture_menu_smoke=pass`、`hotkey_registration_smoke=pass`、`startup_smoke=pass activation_policy=Accessory` | Pass |
| SPIKE-GATE-02 | 丢弃分支 `scripts/check-no-network.sh` | 驻留无网络 | `privacy_smoke=pass resident_pid=865 network_sockets=0 lsof_status=1` | Pass |
| SPIKE-GATE-03 | 丢弃分支 `cargo fetch --locked` 后 `scripts/inspect-dependencies.py` | 依赖来源可审查 | `gpui-pre 0.3.11`/`gpui-pre-platform 0.3.11`、`dependency_sources=path,registry+https://github.com/rust-lang/crates.io-index`、`duplicate_package_names=29`（0.2.2 基线 55，0.3.8 为 43） | Pass |
| SPIKE-GATE-04 | 丢弃分支 `scripts/check-architecture.sh` | 架构门 | `architecture check failed: expected gpui v in app dependency graph` | Fail（预期，#52 改包名断言） |

## 观察到的行为差异（迁移注意）

- 可访问性树只在辅助技术客户端连接后构建（stderr `Accessibility activated`）；窗口刚打开时第一次 `snapshot --app` 可能返回空树，必须重读，不能把首读为空当结论。
- `AXPress` 只触发 `on_a11y_action`。只注册 `on_click` 时点击无任何效果；两者都注册后同一引用才驱动产品队列。
- 元素 `.id()` 是可访问性节点的前提；`.accessibility_id()` 才映射成 macOS `AXIdentifier`；裸 `&str` 文本子节点不上报（`capture-status` 之所以可见，是因为外层 `div` 有 id + `role(Label)` + `aria_label`）。
- 只有可交互节点拿得到 `ref_id`（button 有，`statictext` 没有）。
- 0.3.11 已不再拉 `cocoa 0.26`/`block 0.1.6`（0.3.8 会触发 future-incompat 告警），但 `gpui-pre-macos` 仍打印一行无害 stderr `sandbox_extension_issue_file_to_process failed … (Operation not permitted)`；日志断言不要假设 stderr 为空。

## 限制（Not tested）

- 0.3.10 与 0.3.9 未实测：最新 0.3.11 直接通过，未触发向下回退；不得据本记录声称其可用。
- 中文 IME/组合文字、焦点遍历、AX 文本读写：产品尚无文本输入控件，未验证。
- 标注画布的多对象 AX 暴露策略（逐对象 vs `a11y_synthetic_children`）：未验证。
- 正式 bundle 身份下的 AX 等价性：本次 AX 读取用的是裸二进制（进程名 `arcshot`）与 Debug bundle 的 `--smoke-test`，二者未逐项对照。
- 覆盖层是否仍被 `CGWindowList` 排除、Release 构建、x86_64、签名/公证、性能与资源门槛（PERF-*、RESOURCE-MAX）均未评测。
- 未关闭、未修改任何 issue。#52 才能关闭本组事项。
