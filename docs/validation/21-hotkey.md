# #21 全局快捷键、冲突恢复与会话重启验证记录

本记录不是 MAC-START 全部必测项的通过结论，#21 的本票行为已由自动化与用户实机
手动验收共同覆盖。用户按 A–D 步骤完成实机验收并全部通过：热键进入同一 Capture
Session、会话中重复触发重新冻结、设置窗口录入/Esc/试注册保存、跨重启记忆、启动
注册失败提示。恢复失败路径与输出/保存对话框互斥分别属于不可正常复现项与后续切片，
标注 **Not tested**。未保存取证图像，未截图、未联网。

## 实测来源与环境

- 基线 SHA：`b06c787a`（main）。
- 实测实现/脚本 SHA：`fd62647ff2d4c1a7ac5b857223f5ff0c77330f5a`（含诊断日志；
  其后的纯文档提交不改产品源码）。
- 时间：2026-10-30T18:34:12Z（最终完整入口与证据采集）。
- 构建配置：Debug 与 Release。Rust 自动化为 Debug；两种 bundle 均运行启动/菜单/
  热键注册/退出冒烟。
- 签名：本地自签名身份 `ArcShot Dev`（非正式签名/公证，仅本机测试）。Designated
  requirement 为 `identifier "dev.arcshot.app"（Debug 为 dev.arcshot.app.debug）
  and certificate leaf = H"d86592b863e6afa3530b59f2dc9cf170ea77cb1c"`，不绑定 CDHash。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：native preflight：ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2。
- 权限：Release bundle `dev.arcshot.app` 已授予屏幕录制；经 LaunchServices 启动的
  预检 `screen_recording=true`。从终端直接运行同一二进制时 TCC 将访问归属到启动它
  的终端应用（本机终端未授权）并读回 `false`，该读数不代表 bundle。Debug bundle
  未单独授权或验证。普通热键路径不需要 Accessibility / Input Monitoring。
- 夹具：合成热键策略 fixture v1（默认/备选组合、原生结果注入）；手动项使用真实
  GUI 按键与保存对话框，不含用户截图。
- 日志：`target/21-check-all-final.log`、`target/startup-smoke-debug.log`、
  `target/startup-smoke-release.log`、`target/21-manual.log`、
  `target/21-validation-debug.txt`、`target/21-validation-release.txt`。日志为本地
  构建产物，不入库；本报告保存结果摘要。
- 元数据报告 `worktree_dirty=true` 的唯一原因是任务开始前已存在的未跟踪 `.claude/`
  与 `.agents/`、`AGENTS.md`、`docs/agents/model-pool.md` 的用户改动；产品文件在
  采集与测试时已提交，无产品未提交差异。

## 执行

```sh
scripts/check-all.sh > target/21-check-all-final.log 2>&1
scripts/record-validation.sh --configuration debug \
  --logs target/21-check-all-final.log,target/startup-smoke-debug.log
scripts/record-validation.sh --configuration release \
  --logs target/21-check-all-final.log,target/startup-smoke-release.log

# 权限预检必须经 LaunchServices，避免把终端权限当成 bundle 权限：
open --stdout "$PWD/target/21-preflight.log" \
  target/bundle/release/ArcShot.app --args --check-capture-environment

# 手动验收实例（LaunchServices 启动 + stderr 日志）：
pkill -f 'ArcShot.app/Contents/MacOS/arcshot'
open --stderr "$PWD/target/21-manual.log" target/bundle/release/ArcShot.app
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、**42 个 Rust 集成测试通过**、
doc、验证工具/依赖/架构/真实文字后端、Debug 与 Release 原生驻留/菜单/热键注册/
退出冒烟、驻留 socket 检查（`privacy_smoke=pass network_sockets=0`）全部通过；
两种 bundle 均以 `signing=identity name=ArcShot Dev` 构建。传递依赖
block/proc-macro-error2 的 future incompatibility 提示仍存在，不是当前编译或
Clippy 失败。

双轴代码审查（Standards/Spec 并行子代理）后的修正已包含在上述结果内：Drop 在
handler 移除失败时泄漏按键计数器副本以避免 use-after-free、注销失败的
`EventHotKeyRef` 记入 retired 列表并在 Drop 重试、录入中显示候选组合及合法性、
暂挂失败时退出录入、保存不再先恢复旧注册（避免恢复失败误报）、`is_valid` 文档
补充平台键映射归属、显示/存储共用修饰键表。审查另确认：合成 CGEvent 按键投递
不可作为自动化证据（见下），输出互斥对菜单 Start 同样生效与 I-10“统一输出/
对话框互斥”一致。

签名身份有效性单独验证：固定身份构建在**改变 CDHash** 的重建前后
（`ce26cb67…` → `12409f9a…`，为验证临时改变一条日志字符串后退回）经
LaunchServices 预检均为 `screen_recording=true`，即系统授权跨重建保留；此前
ad-hoc 构建每次重建都会使授权失效，这也是手动验收期间反复丢失权限的原因。

## 稳定自动化子用例

各行前置为上述 SHA、锁定依赖与合成策略 fixture，不需要屏幕录制权限；原生冒烟
例外只读系统状态并注册/注销热键。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-HOTKEY-01 | 默认组合 | Command+Shift+X，合法 | `⌘⇧X`，valid | Pass |
| CORE-HOTKEY-02 | 构造各修饰键组合与无修饰/仅 Shift/控制字符键 | 至少含 Command/Control/Option 之一且为普通键才合法 | 矩阵全部符合，Shift+普通键与裸键不合法 | Pass |
| CORE-HOTKEY-03 | 显示各组合 | 固定顺序 ⌘⌃⌥⇧ + 大写键 | 顺序与大小写正确，空格显示“空格” | Pass |
| CORE-HOTKEY-04 | 存储串往返；注入畸形串 | 往返一致；畸形/非法组合拒绝 | 9 类畸形输入全部 `None`，合法往返一致 | Pass |
| CORE-SESSION-06 | Frozen 后 `SetOutputBusy(true)` 再 Start | 忽略 Start，会话不变；释放后恢复 | 无效果且保持 Frozen，释放后 `DismissOverlay`+新预检 | Pass |
| CORE-SESSION-07 | Frozen → Start → 权限通过 → 新捕获失败 | 丢弃旧底图、回驻留、不恢复旧会话、不提示丢失 | `frame=None`、Resident、`ReportFailure(InvalidFrame)` | Pass |
| APP-EFFECT-04 | `Launch` → `RegisterResult(Applied)` | 注册配置，不写盘 | `Register(config)`，无 `Store` | Pass |
| APP-EFFECT-05 | 启动注册失败 | 提示且保留配置，不假称生效 | `StartupRegistrationFailed`，`registered=None` | Pass |
| APP-EFFECT-06 | 保存备选组合成功 | 替换并持久化 | `Register` → `Store`，config/registered 更新 | Pass |
| APP-EFFECT-07 | 录入中保存失败 | 保留旧配置并恢复旧注册 | `SaveRejected` + `Resume`，旧组合恢复 | Pass |
| APP-EFFECT-08 | 保存失败后恢复也失败 | 明确报错且不得显示生效 | `RestoreFailed`，`registered=None` | Pass |
| APP-EFFECT-09 | 暂挂失败 | 保留注册并退出录入 | `SuspendFailed`，旧组合仍生效 | Pass |
| APP-EFFECT-10 | Esc 取消录入 / 保存未变更组合 | 恢复注册，不保存 | `Resume`；未变更保存无效果 | Pass |
| APP-EFFECT-11 | 录入中保存成功 / 失败 | 成功只替换并持久化，不恢复旧注册；失败恢复一次 | `Store`；`SaveRejected`+`Resume` | Pass |
| MAC-HOTKEY-21-01 | `--smoke-test`（Debug 与 Release）注册默认 ⌘⇧X、suspend、resume、Drop | 原生注册/暂挂/恢复/清理成功，未启动会话 | `hotkey_registration_smoke=pass default=⌘⇧X suspend_resume_cleanup=clean` | Pass |

## 用户实机手动项（MAC-START 子集）

前置：Release bundle `dev.arcshot.app`，固定签名 `ArcShot Dev`，屏幕录制已授权，
经 LaunchServices 启动且 stderr 写入 `target/21-manual.log`。

| ID | 步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| MAC-START-21-MANUAL-01 | 按已保存的 `⌃⌥A` | 进入同一 Capture Session 并冻结内屏 | 出现冻结画面；Esc 回驻留 | Pass |
| MAC-START-21-MANUAL-02 | 冻结中再按 `⌃⌥A` | 丢弃旧画面重新冻结，不提示丢失 | 直接重新冻结，无任何提示 | Pass |
| MAC-START-21-MANUAL-03 | 设置窗口：点击录入、`⇧X` 与裸键、Esc、保存 `⌃⌥A` | 仅录入区接键；非法组合不可保存；成功后才替换 | 行为符合；日志为 `hotkey_recorded` → `hotkey_save_pushed` → `hotkey_save_requested` → `hotkey_register outcome=Applied` → `hotkey_store value=⌃⌥A` | Pass |
| MAC-START-21-MANUAL-04 | 保存后完全退出并重启，读 `defaults read dev.arcshot.app GlobalCaptureHotkey` | 保存值保留，启动即注册 | `control+option+a`；日志 `hotkey_launch config=⌃⌥A` + `hotkey_register outcome=Applied` | Pass |
| MAC-START-21-MANUAL-05 | 注入 `option+ü` 后启动 | 明确提示且不显示为生效，菜单入口仍可用 | 日志 `hotkey_operation_failure=the key has no ANSI virtual keycode` + `hotkey_register outcome=Failed`；出现“全局快捷键 ⌥Ü 注册失败…”提示；菜单“开始截图”仍可用 | Pass |
| MAC-START-21-MANUAL-06 | 保存时旧注册恢复失败 | 明确报错且不得显示为生效 | 正常使用不可复现（需原生注销失败）；APP-EFFECT-08 自动化覆盖该状态机 | Not tested |
| MAC-START-21-MANUAL-07 | 输出处理/保存对话框期间按热键 | 忽略 Start，不开始新会话 | 输出动作属后续切片，本票仅落地共享合同；CORE-SESSION-06 覆盖 | Not tested |

## 原生冒烟与已知限制

- `--smoke-test` 同时验证：三个真实菜单 action 入队（`queued_commands=3`）、
  Accessory 策略、状态项图像/菜单/`terminate:`、退出 exit 0、5 秒看门狗内完成；
  Release bundle 同样通过。
- 合成 CGEvent（`CGEventCreateKeyboardEvent` + `CGEventPost`，含投递到自身 pid 与
  HID tap 两种方式）未触发已注册热键，因此按键投递只保留人工证据；合成键盘事件
  在本机不参与热键匹配，不构成对功能失败的证据。
- 终端直接运行二进制时，TCC 把屏幕录制访问归属到启动它的终端应用；权限结论必须
  用 LaunchServices 启动的预检或 app 内行为取得。
- 恢复失败路径、输出/保存对话框互斥与系统级热键冲突的人工复现未执行；前三者由
  自动化状态机覆盖，冲突在非独占注册语义下不承诺系统独占。
