# #21 全局快捷键、冲突恢复与会话重启验证记录

本记录不是 MAC-START 完整通过结论，#21 保持开放，PR 不自动合并。真实按键
投递、系统冲突、设置窗口人工操作与偏好跨重启需要用户实机参与；注册/暂挂/
恢复/清理与全部纯策略行为已有自动化证据。未保存取证图像，未截图、未联网。

## 实测来源与环境

- 基线 SHA：`b06c787a`（main）。
- 实测实现/脚本 SHA：`5e30db06c49cd39b711994adba0e47e5c646778f`。
- 时间：2026-10-30T17:08:08Z（环境采集与完整入口执行；实现 SHA 之后未再改产品源码）。
- 构建配置：Debug 与 Release。Rust 自动化为 Debug；两种 bundle 均运行启动/菜单/
  热键注册/退出冒烟。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB。
- macOS：27.2 (26B123)。
- 显示器：本次 native preflight（Debug bundle）：ID=1，global origin=(0,0)，
  1470×956 逻辑点，scale=2。
- 权限：本次 Debug bundle `screen_recording=false`、`guide_seen=false`（未请求或
  改动权限；本票不新增任何权限，普通热键路径不需要 Accessibility / Input Monitoring）。
- 夹具：合成热键策略 fixture v1（默认/备选组合、原生结果注入）；无真实按键或
  用户截图参与。
- 日志：`target/21-check-all.log`、`target/startup-smoke-debug.log`、
  `target/startup-smoke-release.log`、`target/21-capture-environment.log`、
  `target/21-validation-debug.txt`、`target/21-validation-release.txt`。日志为本地
  构建产物，不入库；本报告保存结果摘要。
- 元数据报告 `worktree_dirty=true` 的唯一原因是任务开始前已存在的未跟踪 `.claude/`
  与 `.agents/`、`AGENTS.md`、`docs/agents/model-pool.md` 的用户改动；产品文件在
  采集与测试时已提交，无产品未提交差异。

## 执行

```sh
scripts/check-all.sh > target/21-check-all.log 2>&1
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot --check-capture-environment
scripts/record-validation.sh --configuration debug \
  --logs target/21-check-all.log,target/startup-smoke-debug.log
scripts/record-validation.sh --configuration release \
  --logs target/21-check-all.log,target/startup-smoke-release.log
```

完整入口 exit 0：fmt、Clippy warnings-as-errors、**41 个 Rust 集成测试通过**、
doc、验证工具/依赖/架构/真实文字后端、Debug 与 Release 原生驻留/菜单/热键注册/
退出冒烟、驻留 socket 检查（`privacy_smoke=pass network_sockets=0`）全部通过。
传递依赖 block/proc-macro-error2 的 future incompatibility 提示仍存在，不是当前
编译或 Clippy 失败。

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
| MAC-HOTKEY-21-01 | `--smoke-test`（Debug 与 Release）注册默认 ⌘⇧X、suspend、resume、Drop | 原生注册/暂挂/恢复/清理成功，未启动会话 | `hotkey_registration_smoke=pass default=⌘⇧X suspend_resume_cleanup=clean` | Pass |

## 原生冒烟与人工项

- `--smoke-test` 同时验证：三个真实菜单 action 入队（`queued_commands=3`）、
  Accessory 策略、状态项图像/菜单/`terminate:`、退出 exit 0、5 秒看门狗内完成；
  Release bundle 同样通过。
- 尝试过以合成 CGEvent（`CGEventCreateKeyboardEvent` + `CGEventPost`）自动验证
  按键投递：事件未触发已注册热键（handler 未被调用），因此按键投递不作自动化
  结论；合成键盘事件在本机不参与热键匹配，不构成对功能失败的证据。
- 以下为 MAC-START 用户参与项，当前 **Not tested**：真实 ⌘⇧X 按键进入同一
  Capture Session（含 Esc/Enter/双击等既有按键）；会话中重复触发重新冻结；
  菜单“截图快捷键…”窗口的点击录入、Esc 取消、试注册保存；非法组合/注册冲突
  提示与旧配置保留；偏好跨重启记忆；启动注册失败提示；菜单入口始终可用。
- 手动步骤建议：以 Debug 或 Release bundle 启动（终端直接运行会继承终端权限，
  与 bundle 的 TCC 身份不同），从菜单打开“截图快捷键…”，先试 `⇧X`（应提示
  不合法），再保存 `⌘⌃A`，重启后确认仍为 `⌘⌃A`，最后改回默认并确认。
