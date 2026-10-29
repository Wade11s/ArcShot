# #19 启动与退出验证记录

本记录只覆盖菜单栏驻留切片，不是 ArcShot V1 发布验收。

## 来源与环境

- 基线 SHA：`47355088b8c3ebda5e588ca62c3ad576e8e8fe16`；变更最终 SHA 在 PR 证据中给出。
- 时间：2026-10-29T12:55:29Z（UTC，环境采集）；完整套件于 2026-10-29T13:01:38Z 完成。
- 机器：Mac14,2，Apple M2，16 GB，arm64。
- 系统：macOS 27.2 (26B123)。
- 内建显示器：1470×956 逻辑点，`backingScaleFactor=2.0`；面板报告 2560×1664 Retina。无捕获或跨屏行为测试。
- Rust/Cargo：1.99.1（rustc `58cf8f9c35b355199727627d07ae34a4e2846d67`）。
- Xcode：27.0 (27A266a)；SDK 27.0 (26A425)。
- Swift：6.4 (`swiftlang-6.4.0.34.1`)；Metal/AIR-LLD：32023.921。
- 构建配置：Debug 与 Release 均完成原生冒烟。
- 权限：本切片不捕获、不监听输入，未请求 Screen Recording、Input Monitoring、Accessibility 或通知权限。原生 action 由进程内 AppKit 驱动，不使用 System Events。TCC 数据库内容未读取，记为 Not tested，而非推断授权状态。
- 夹具：仅内置 36×36 RGBA crop PNG，SHA-256 `7e9bb228f21f62f62a236c61fd509b696d502695d44f9d76fc95a28acde745ea`；无外部图像或网络 fixture。

## 稳定用例

| ID | 前置与步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| ARCH-01 | 在锁定工作区运行 `scripts/check-architecture.sh` | app → core/image/macos；image/macos 仅依赖 core 的中性数据；GPUI 仅在 app | Cargo dependency graph 全部满足 | Pass |
| TEXT-01 | 运行 `scripts/check-text-backend.sh` | 使用真实 macOS 文字后端，不用 NoopTextSystem | Helvetica 16 px：`i=3.5546875px`，`W=15.1015625px`，满足 `W > i` | Pass |
| MAC-START-01 | 从 `target/bundle/debug/ArcShot.app` 内二进制运行 `--smoke-test` | 无 Dock Accessory；真实可见状态项、图像、菜单；菜单自身 `terminate:` 排队执行并 exit 0；5 秒内完成 | 修复后连续四次输出 `startup_smoke=pass activation_policy=Accessory quit_action=terminate:`，exit 0 | Pass |
| PRIVACY-01 | 正常启动、驻留 0.5 秒，执行 `lsof -nP -a -p PID -i`，随后结束该测试进程 | 进程仍驻留，网络/监听 socket 数为 0 | `privacy_smoke=pass network_sockets=0` | Pass |
| BUILD-RELEASE-01 | 构建 Release bundle 并执行同一 MAC-START-01 | Release 也完成驻留与退出 | Release 构建成功，`startup_smoke=pass activation_policy=Accessory quit_action=terminate:`，exit 0 | Pass |
| MAC-START-MANUAL-01 | 用户从 Finder 启动，观察菜单栏图标和 Dock，点击“退出 ArcShot” | 图标可见、无 Dock 主窗口，点击后进程退出 | 需要用户实机视觉与点击；自动 AppKit 检查不替代 | Not tested |

## 缺陷回归与限制

- 初次退出冒烟在 GPUI `App` 借用内同步 `terminate:`，重入 `will_terminate` 并触发 `RefCell already borrowed` / abort。修复为经 `dispatch2::MainThreadBound` 回到主队列后执行同一 action；原红色脚本转绿并连续重复通过。
- `lsof` 是本次驻留时的进程快照，不是对后续功能或所有系统活动的永久证明。应用自身未调用 HTTP、监听、回环或截图 API。
- 没有捕获、保存或生成用户截图；唯一 PNG 是源码内的菜单图标。构建与测试日志仅文本，保存在 `target/`。
- 未测试正式签名、公证、中文 IME、截图权限、全局热键、截图/标注及后续 V1 功能；这些功能没有以可点击假命令出现。
