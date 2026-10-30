# #20 权限与冻结 Capture Session 验证记录

本记录不是完整产品原生验收通过结论，#20 保持开放，PR 不自动合并。后续人工
进展已取得 Release 权限可用、真实冻结及 Esc 的子集证据；初期预览清晰度问题
修复后已通过用户复测。其余矩阵不得视为通过。捕获由用户在实机步骤中触发，未保存取证图像。

## 实测来源与环境

- 基线 SHA：`0f968d33e4901c4f28d6e2f4679571625da93a6f`。
- 实测实现/脚本 SHA：`4b997be808007adc9432a5a8eb916059692f7085`。
- 时间：2026-10-29T16:52:28Z（环境采集）；完整入口随后未再改产品源码。
- 构建配置：Debug 与 Release。Rust 自动化为 Debug；两种 bundle 都运行启动/菜单/退出冒烟。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)；cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)。
- SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal/AIR-LLD：Apple metal 32023.921 / AIR-LLD 32023.921（均为 `metalfe-32023.921.6`）；补充运行 `xcrun -sdk macosx metallib --version` 确认 linker，无产品代码改动。
- 机器：Mac14,2，arm64，Apple M2 / 16 GB（沿用本机 #19 硬件信息）。
- macOS：27.2 (26B123)。
- 显示器：本次原生 preflight：ID=1，global origin=(0,0)，1470×956 逻辑点，scale=2；预期捕获 2940×1912 物理像素，未实际捕获。
- 权限：本次 bundle 的 Screen Recording preflight=true；guide_seen=false。未请求或改动权限，未检查/请求 Input Monitoring、Accessibility 或通知。
- 夹具：实测 SHA 中的合成 opaque-BGRA/padded 2×2 与 2940×1912 内存 fixtures v1；worker 通道/时钟/异常注入 v1。不是用户截图或自然内容 oracle。
- 日志：`target/20-check-all.log`、`target/20-native-preflight.log`、
  `target/20-capture-environment.log`、`target/startup-smoke-debug.log`、
  `target/startup-smoke-release.log`、`target/resident-smoke.log`、
  `target/20-validation-debug.txt`、`target/20-validation-release.txt`。
  日志为本地构建产物，不入库；本报告保存结果摘要。
- 元数据报告 `worktree_dirty=true` 的唯一原因是任务开始前已存在的未跟踪
  `.claude/`；产品文件在采集和最后一轮测试时已提交，无产品未提交差异。
  本报告本身是之后的纯证据文档提交。

## 执行

开发中按单文件运行，首个 tracer 及若干新增行为观察过 red → green，包括坐标、
帧接纳、权限策略、绝对期限，以及审查发现的旧 Esc/排队重启回归。

最终实现 SHA 上执行：

```sh
scripts/check-all.sh > target/20-check-all.log 2>&1
cargo test --locked -p arcshot-macos --test native_preflight -- --nocapture
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot --check-capture-environment
scripts/record-validation.sh --configuration debug \
  --logs target/20-check-all.log,target/startup-smoke-debug.log,target/resident-smoke.log
scripts/record-validation.sh --configuration release \
  --logs target/20-check-all.log,target/startup-smoke-release.log
```

完整入口 exit 0，**27 个 Rust 集成测试通过**，无失败；fmt、Clippy
warnings-as-errors、doc、验证工具/依赖/架构/真实文字后端、Debug/Release
原生驻留/退出与驻留 socket 检查通过。另在修复第二个输入竞态前运行过完整入口，
该轮不作为最后实现的证据。传递依赖 block/proc-macro-error2 的 future
incompatibility 提示仍存在，不是当前编译或 Clippy 失败。

## 稳定自动化子用例

各行前置为上述 SHA、锁定依赖及合成夹具，不需要屏幕权限。原生预检例外只读系统状态。

| ID | 前置/步骤 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| CORE-COORD-01 | 构造 (-1470,40) 的 1470×956@2x 几何 | sourceRect 从 (0,0) 开始；物理 2940×1912；global origin 独立 | 三者独立正确 | Pass |
| CORE-COORD-02 | 注入 NaN/Inf、非2x、零/分数/超尺寸与总像素超预算 | 在转换/分配前拒绝 | 全部返回错误 | Pass |
| APP-FRAME-01 | 接纳有行 padding 的 opaque BGRA 2×2 | 保持像素和 stride；padding 不参与 alpha | stride=12，像素正确 | Pass |
| APP-FRAME-02 | 注入尺寸、stride、字节数、乘法溢出、alpha 与预算错误 | 接纳前分类拒绝 | 六类错误均拒绝 | Pass |
| CORE-SESSION-01 | Start → 权限结果 | 先预检，成功才 Capture | CheckPermission → Capture | Pass |
| CORE-SESSION-02 | 接纳帧 → Cancel | Frozen 可见；取消清底图，无输出效果 | Resident 且 frame=None | Pass |
| CORE-SESSION-03 | preflight=false 后再次 Start | 不捕获；提示不可用；下次重检 | 权限恢复效果及新检查正确 | Pass |
| CORE-SESSION-04 | Frozen → DisplayInvalidated | 清帧并报显示失效，不迁移 | 底图不可访问、旧呈现票据失效 | Pass |
| CORE-SESSION-05 | 已知 frame 错误 → Start | 允许新权限检查及安全重试 | 新 ticket，无旧帧 | Pass |
| APP-ASYNC-01 | Preparing → Cancel → 成功迟到帧 | Draining 等确认，不呈现迟到帧 | 返回 Resident，frame=None | Pass |
| APP-ASYNC-02 | Preparing → Start → 安全完成 | 不并行原生请求；完成后才新预检 | 新 ticket 仅在完成后产生 | Pass |
| APP-ASYNC-03 | Cancel 后请求不确定/worker panic | 不输出、不重启；迟到成功不恢复 | RelaunchRequired 保持 | Pass |
| APP-ASYNC-04 | 取消后返回旧 preflight | 不发原生任务 | 空效果、may_capture=false | Pass |
| APP-ASYNC-05 | 注入不返回操作，推进到提交后5秒 | 应用绝对期限终止；任何后续 submit 被拒；迟到无效果 | NativeUncertain 且 gate 终态 | Pass |
| APP-ASYNC-06 | 捕获闭包 panic | WorkerPanicked，禁止重启 | worker 边界捕获并进入终态 | Pass |
| APP-ASYNC-07 | 在途通道阻塞/已知错误完成 | 忙时拒绝；worker 退出后才允许重试 | 无并行，普通结果后可重新提交 | Pass |
| APP-ASYNC-08 | Frozen 旧 Esc 排在新菜单 Start 后 | 旧输入不取消新 Preparing | 旧 ticket 无效果，新 ticket 可取消 | Pass |
| APP-ASYNC-09 | Preparing → Start → 旧 Esc → 完成 | 不抹掉排队重启；不确定结果仍终态 | 安全结果新预检，错误禁止重启 | Pass |
| APP-EFFECT-01 | 首引导 → 稍后 → 注入重启偏好 | 写偏好；重启不自动再弹，菜单仍预检 | 对应 effects 正确 | Pass |
| APP-EFFECT-02 | 配置 → 返回设置 → false/true 重检 | 仅显式意图请求；返回检查实际结果，不启动 capture | notice/效果正确 | Pass |
| APP-EFFECT-03 | 按需不可用 → 配置/取消 | 可请求并打开设置、重检/取消；无额外权限 | 所需效果且取消后无返回检查 | Pass |
| IMAGE-LIMIT-01 | 有 padding 的合成帧生成预览 | BGRA 不变、去 padding，不改底图 | 与独立字节 literal 一致 | Pass |
| IMAGE-LIMIT-02 | 2940×1912 底图生成预览 | 预览≤4M，原始2x尺寸不变 | 预览1470×956，底图2940×1912 | Pass |
| MAC-PREFLIGHT-20-01 | 原生测试注入陈旧 display ID=0 | 首次 SCK 前拒绝，不捕获 | 已知 Display 错误 | Pass |
| MAC-PREFLIGHT-20-02 | CoreGraphics 内屏预检 | 唯一内屏2x或明确已知失效 | 本机实际1470×956@2x | Pass |
| MAC-PREFLIGHT-20-03 | 构造原生错误分类 | Request 不确定；普通显示/帧/预算错误已知 | 分级符合合同 | Pass |
| MAC-PREFLIGHT-20-04 | worker 调用 AppKit-only 接口 | 拒绝越线程窗口枚举，不 panic | NotMainThread；active=false | Pass |
| MAC-MENU-20-01 | 正式 Debug/Release 冒烟发送实际 NSMenu 两个 action，不启动 coordinator | 两个命令依次入队，不请求权限或捕获 | queued_commands=2，no_native_capture=true | Pass |
| MAC-START-01 / BUILD-RELEASE-01 | 原生 bundle 启动、验证、排队 quit | 无 Dock Accessory、真实状态项、退出0 | 两种配置均通过 | Pass |
| PRIVACY-01 | Debug 正常驻留，lsof 查询 | 无网络/监听 socket；异常不假通过 | network_sockets=0，lsof_status=1 | Pass |

## 原生验收缺口

下列前置/步骤/预期完整定义在 `docs/capture.md`，本轮均未执行；只列实际，不以
合成或预检代替产品捕获。

| ID | 前置/步骤与预期定位 | 实际 | 结果 |
| --- | --- | --- | --- |
| MAC-PERMISSION-MANUAL-20-01 | 首引导稍后/跨重启；docs/capture.md | 无真人操作，未修改真实偏好 | Not tested |
| MAC-PERMISSION-MANUAL-20-02 | 权限不可用、系统设置/返回重检；docs/capture.md | 只有可用预检，没有权限切换 | Not tested |
| MAC-REGION-MANUAL-20-01 | 真实捕获、冻结、2x与无自有 UI；docs/capture.md | 未调用 SCK 捕获像素 | Not tested |
| MAC-REGION-MANUAL-20-02 | Esc 清理及再次捕获；docs/capture.md | 仅合成状态与输入调度验证 | Not tested |
| MAC-TARGET-MANUAL-20-01 | 内屏几何/拓扑失效；docs/capture.md | 仅 core 失效结果注入 | Not tested |
| MAC-NATIVE-MANUAL-20-01 | 真实原生错误/超时恢复；docs/capture.md | 仅 worker/时钟注入 | Not tested |
| MAC-COLOR-MANUAL-20-01 | 颜色、方向、精确窗口排除全链路；docs/capture.md | 未执行 native oracle，不引用原型结果 | Not tested |
| PERF-SCK / PERF-REGION | Release 预热+30样本，真实呈现测量 | 已增加捕获与窗口提交诊断，无实际捕获样本；窗口提交不等于首次呈现 | Not tested |
| RESOURCE-MAX | footprint/RSS/native/GPU观测 | 只有分配前预算规则，没有压力测量 | Not tested |

未实现/不验收选区、编码输出、全局热键和 Scroll Capture。无输出接口意味着本
切片不会写剪贴板/截图文件，但不是对后续输出行为的验证。没有取证图像落盘。

## 双轴审查及派遣

基准为上述基线 SHA。Standards 和 Spec 分开独立审查，非跨模型互证：

- **Standards**：发现旧覆盖层 Esc 取消新会话及 Preparing 的旧输入抹掉排队重启
  两种 P2 时序，均已修复并增加 APP-ASYNC-08/09；最终 0 未决规范问题、
  0 actionable smell。
- **Spec**：最终 0 未决代码问题、0 范围蔓延；保留上述原生证据缺口作为验收阻塞，
  不把它合并成代码问题或报告为通过。
- 实现代理 `7ec568`：实际 `openai-codex/gpt-6.1-terra` / high；
  首选 deepseek-v4.1-flash 在 `pi --list-models` 不可用，使用实施池备选。
  仅在 `ticket/20-native-capture`、`.worktrees/20-native` 修改 macos、
  Cargo.lock、docs/build.md；运行 package check/clippy/doc/preflight 与启动冒烟，
  未捕获屏幕。提交已 cherry-pick，临时 worktree/分支已清理。
- Standards `77defe`、Spec `ecb33b`：均实际
  `openai-codex/gpt-6.1-sol` / high，无降级；当前 ticket 分支只读。
  最终各自执行相关 Rust 接缝测试（27通过）及 diff 检查；Standards 另跑 workspace
  check。完整 scripts/check-all.sh 由主代理执行，不冒称审查代理跑过完整入口。
- 三个代理均已 stop。原生授权、真人 UI/捕获、颜色与资源风险仍如上表。

## 2026-10-30 检修：权限旧输入与状态栏代理窗口

本节是上述历史记录之后的独立运行，不改写旧报告。完整入口再次 exit 0，
27 个 Rust 集成测试通过，Debug/Release 原生启动、菜单、窗口枚举及驻留 socket
检查通过；仍没有调用 SCK 读取像素或保存截图，#20 的真实捕获验收保持开放。

### 实测快照

- 基线 SHA：`d9564d875a6bc86f8e303099e51c0561f7e0080f`；本次修复未提交，
  `worktree_dirty=true`，不能把基线 SHA 单独当作包含修复的实现版本。
- UTC：2026-10-30T12:24:12Z（Debug 环境采集）、2026-10-30T12:24:13Z（Release）。
- 构建配置：Debug 与 Release。权限窗口竞态使用 Debug 真实 GPUI/AppKit 窗口，
  不使用 mock 窗口或原型。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)、cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)；SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64；macOS：27.2 (26B123)。
- 显示器：ID=1，origin=(0,0)，1470×956 逻辑点，scale=2；未捕获物理像素。
- 权限：最终 Debug bundle 的 `screen_recording=false guide_seen=false`。
  未调用权限请求、打开系统设置或更改持久偏好；不沿用旧记录的可用权限结论。
- 夹具：真实状态项、权限提示及 GPUI Esc 分发 v1；权限策略以 Handled 初始化，
  不重置/写入真实首引导偏好。临时复现模块和 CLI 入口在完整检查前已移除。
- 日志：`target/maintenance-check-all.log`、`target/startup-smoke-debug.log`、
  `target/startup-smoke-release.log`、`target/resident-smoke.log`；环境元数据为
  `target/maintenance-validation-debug.txt`、`target/maintenance-validation-release.txt`。
  竞态的临时原生运行结果保留在下表和输出摘要中，不声称有常驻自动化用例。
- 未提交差异：本次产品改动为 app 的 coordinator、ui、main 和 macos 的 capture；
  同步更新 TEST.md、docs/capture.md 及本报告。任务开始前已有 AGENTS.md、
  `.agents/skills/arcshot-delegation/SKILL.md`、docs/agents/model-pool.md 与 `.claude/`
  的用户改动，未改动这些文件。

完整检查使用的四个源码文件 SHA-256（格式化后、无临时入口）：

| 文件 | SHA-256 |
| --- | --- |
| `crates/app/src/coordinator.rs` | `53a9c1e2ad63ca901dd59c079e3914dbe55d704ed0362b556fc2740aab1470f9` |
| `crates/app/src/ui.rs` | `b2e701f840d2e254f9aaae1336ce9e4fabd2461a44e1ac16691160fbcce3aa9e` |
| `crates/app/src/main.rs` | `145c821471b8a9aeed8e1016b399f0df6026df3b9422753c493e1097d313d568` |
| `crates/macos/src/capture.rs` | `58ca0f48cd734ede524d372fcf00fce02fc46a531cb98c9279e9a2f0d48fb4c4` |

### 故障与行为证据

| ID | 前置/步骤 | 预期 | 修复前实际 | 修复后实际 | 结果 |
| --- | --- | --- | --- | --- | --- |
| MAC-PERMISSION-QUEUE-20-01 | 真实权限提示接收 Esc，消费队列前以 Check 替换提示，再执行 coordinator tick | 旧 Esc 不关闭新提示；新提示自己的 Esc 可关闭 | `stale Esc dismissed replacement permission notice`，原生复现 exit 1 | 旧 Esc 被忽略，当前 Esc 正常关闭；500 ms 后 GPUI 窗口=0、原生窗口列表为空 | Pass |
| MAC-WINDOW-20-01 | 正式 Debug/Release 状态项进程执行 `scripts/check-startup.sh` | 状态栏代理不成为可捕获窗口；枚举为空，正常退出 | `NSStatusBarWindow.windowNumber=4294967296` 且 isVisible=true；u32 转换拒绝，启动回归 exit 1 | 官方屏幕窗口枚举返回空列表，两种配置均通过 | Pass |
| MAC-WINDOW-20-02 | 临时真实权限窗口运行调用自有窗口枚举；关闭后再次枚举 | 返回实际窗口编号；不被状态栏代理阻塞，窗口销毁后列表为空 | 隐藏状态项前后都得到 `CaptureError::Window` | 返回真实窗口编号；当前 Esc 后清理为空 | Pass |

窗口编号的诊断同时观察到：`NSApplication.windows()` 含有不可用于 CGWindowID 的
状态栏代理；`NSWindow.windowNumbersWithOptions(AllSpaces)` 返回真实自有屏幕窗口。
后者仍逐一检查非零、u32 范围与重复，并由 SCK 的精确 ID/owner 匹配继续拒绝错误。
状态项现于枚举前隐藏，原生准备失败时立即恢复；未增加截断编号、忽略映射错误或
备用捕获路线。

权限输入使用 GPUI WindowId 在消费副作用前核对当前提示。临时原生运行输出摘要：

```text
permission_queue_smoke=pass stale_escape_ignored=true replacement_notice=true no_capture=true
cleanup_gpui_windows=0 native_ids=Ok([])
permission_queue_cleanup_smoke=pass current_escape=true visible_owned_windows=0
```

### 审查与限制

- Standards：独立只读审查确认权限旧输入 P2；已修复并取得上述原生失败/通过证据。
  状态栏代理问题由主代理原生冒烟发现并修复，新增常驻启动回归。
- Spec：独立只读审查未发现 HEAD 原补丁中额外可证明的代码问题；没有把已知的
  实机证据缺口判为通过。两个 reviewer 均未执行本轮完整入口，检查由主代理运行。
- 完整检查继续报告传递依赖 block/proc-macro-error2 的 future incompatibility 提示，
  当前 Clippy 与构建通过。
- 真实 Screen Recording 恢复、SCK 帧、冻结图颜色/方向、精确像素排除、冻结态
  Esc/重启、显示失效与性能矩阵仍为 Not tested。本轮权限提示 Esc 不能替代冻结态 Esc。
- #21 的 GitHub 原生阻塞依赖仍为开放的 #20；没有关闭票据、合并 PR 或声称 V1 放行。

### 人工验收进展（用户反馈）

本节记录用户操作的结果；不把用户视觉确认扩展为像素、原生色彩或性能证明。
原计划使用同一个 Release bundle，但第 3 轮失败后的进程取证显示实际运行的是
`target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot`；第 1/2 轮未核实进程路径，
其行为反馈保留，不能作为已确认 Release 配置的证据。第 1 轮后 Release CLI 只读预检为
`display_id=1 origin=0,0 logical=1470x956 scale=2 screen_recording=false guide_seen=true`，
该 CLI 结果不证明 GUI 实例的构建配置。

| ID | 用户执行步骤 | 用户反馈与实际 | 结果 |
| --- | --- | --- | --- |
| MAC-PERMISSION-MANUAL-20-01 | 首引导出现 → 稍后再说 → 检查菜单 → 退出 → 重开（当时未核实运行路径） | 用户确认全部符合预期：引导可关闭，菜单项完整，退出后图标消失，重开不重复引导 | Pass |
| MAC-PERMISSION-MANUAL-20-02（不可用/重检/取消子集） | 开始截图 → 不可用提示 → 重新检查 → 取消 → 权限菜单再次检查并取消 | 用户确认全部符合预期：未冻结，有设置/重检/取消，重检不重复系统授权请求，两入口均可取消返回驻留 | Pass |
| MAC-PERMISSION-MANUAL-20-02（授权后恢复） | 用户在系统设置完成授权，返回并点击重新检查 | 用户反馈 D：仍显示不可用，重新检查也不可用；完整权限恢复用例未通过 | Fail |

第 3 轮失败后实际进程为 PID 38747，启动于本地 2026-10-30 20:50:17，路径为上述
Debug bundle。Debug/Release 的 bundle ID 同为 `dev.arcshot.app`，均为 ad-hoc 签名；
CDHash 分别为 `9fa62d145486b45a0f631be88835630ac2ee1689` 与
`c38000f98cad87a05a29db0b057f3b2054070401`。

同一 Debug bundle 重启后，用户反馈 B：仍不可用。系统 TCC 日志于本地
2026-10-30 20:55:23.370 明确报告：

```text
Failed to match existing code requirement for subject dev.arcshot.app and service kTCCServiceScreenCapture
    cdhash H"c38000f98cad87a05a29db0b057f3b2054070401"
    cdhash H"9fa62d145486b45a0f631be88835630ac2ee1689"
```

授权校验要求与 Release 哈希一致，Debug 哈希不同。关闭 Debug 后，使用
LaunchServices 明确新建 Release 进程，未重编译、重签名、请求/重置任何权限。
`target/permission-release-preflight.log` 实际输出：

```text
capture_environment display_id=1 origin=0,0 logical=1470x956 scale=2 screen_recording=true guide_seen=true
```

随后同样明确启动 Release GUI，核实仅 PID 39608 运行，路径为
`target/bundle/release/ArcShot.app/Contents/MacOS/arcshot`。GUI 文本日志连接到
`target/manual-release.log`、`target/manual-release-error.log`，供后续实机步骤使用。
用户随后确认 Release GUI「屏幕录制权限…」已显示可用；原生预检与 GUI 提示恢复
均有证据，并继续触发下述两次真实捕获。完整权限恢复矩阵尚未完成，第 3 轮原始
Fail 保留为构建身份不匹配的实际失败。

| ID | 步骤 | 实际 | 结果 |
| --- | --- | --- | --- |
| MAC-PERMISSION-MANUAL-20-02（匹配构建恢复子集） | 关闭签名不匹配的 Debug，明确启动授权匹配的 Release，再从权限菜单检查 | LaunchServices 预检为 true，PID 39608 路径为 Release，用户确认界面已显示权限可用；未重置权限 | Pass |
| MAC-REGION-MANUAL-20-01（基本呈现子集） | 文本编辑显示测试 A、方向文字和彩色圆点，菜单开始截图，观察冻结画面与自有准备窗口/菜单残留 | 用户确认本轮检查均正常，但报告冻结画面像素不清晰；原始捕获日志为 2940×1912、stride=11760、scale=2 | Pass |
| MAC-REGION-MANUAL-20-02 | 首次冻结后 Esc，等待不重现；修改 A 为 B，再次捕获并 Esc | 用户确认两次取消均正常，新捕获显示 B，无旧帧重现 | Pass |
| PREVIEW-QUALITY-20-01 | 比较冻结预览与原屏清晰度 | 用户报告冻结画面像素不清晰；该次测试版本采用 4M 上限的整数最近邻采样，本机 2940×1912 会生成 1470×956 预览，再铺满 2x 屏幕 | Fail |

上述 Pass 只覆盖用户执行的步骤，不包含独立颜色/alpha oracle、动态背景冻结、
显示失效或真实原生故障。原始底图与预览分开保留；此次清晰度问题定位于预览
缩小路径。该次测试时尚未修改呈现策略，也不据此声称未来编码输出已经验证。

`target/manual-release-error.log` 的两次真实捕获文本诊断：

```text
capture_request_us=132593 display=1 pixels=2940x1912 stride=11760 scale=2
capture_wake_window_submit_us=367054 pixels=[2940, 1912]
capture_request_us=23740 display=1 pixels=2940x1912 stride=11760 scale=2
capture_wake_window_submit_us=209355 pixels=[2940, 1912]
```

这些仅为两次样本，窗口提交也不是首次呈现；不判定 PERF-SCK/PERF-REGION 达标。

## 原尺寸冻结呈现修复与复测构建

### 用户确认的规格调整

用户在本次会话选择「原尺寸冻结画面」：普通 Region Capture 冻结底图保留原始
物理分辨率，4M 上限继续用于缩略预览（含滚动缩略预览）。已同步修改
[#18](https://github.com/Wade11s/ArcShot/issues/18) 的 I-03/I-13；24M 图像、16384
单边和 2 GiB 总预算不变。全尺寸冻结表面及 GPUI 上传/纹理包含在既有 8 个全尺寸
表面预算内；272 MiB 留作缩略预览/运行时余量。这不是最大文档 footprint 实测。

image 的冻结呈现改为逐行复制有效 BGRA 像素，只移除 padding，不采样/缩放底图。
没有增加依赖、第二条捕获路径或截图落盘。旧 4M 冻结尺寸断言已由单像素细线与
边缘像素回归替换，不保留相互矛盾的旧合同。

### 实测快照与范围

- 基线 SHA：`d9564d875a6bc86f8e303099e51c0561f7e0080f`，`worktree_dirty=true`；
  本次修复仍未提交，以下文件 hash 确认完整检查使用的实现。
- UTC：2026-10-30T13:20:10Z（Debug）、2026-10-30T13:20:11Z（Release）环境采集。
- 构建配置：Debug 与 Release；Rust 集成回归和合成原生窗口冒烟使用 Debug。
- Rust/Cargo：rustc 1.99.1 (`58cf8f9c3` 2026-10-13)、cargo 1.99.1 (`5f94df478` 2026-08-27)。
- Xcode：27.0 (27A266a)；SDK：macOS 27.0 (26A425)。
- Swift：Apple Swift 6.4 (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)。
- Metal：Apple metal 32023.921 (`metalfe-32023.921.6`)。
- 机器：Mac14,2，arm64；macOS：27.2 (26B123)。
- 显示器：ID=1，origin=(0,0)，1470×956 逻辑点，scale=2。
- 权限：新 Release 经 LaunchServices 执行无捕获预检，返回
  `screen_recording=false guide_seen=true`；未重置或请求系统权限。
- 夹具：不透明 BGRA 黑底、奇数坐标单物理像素白线及末尾通道标记 v1；原生冒烟
  使用合成帧走真实 coordinator、GPUI RenderImage、冻结窗口及 Esc，未调用 SCK。
- 日志：`target/frozen-resolution-check-all.log`、`target/frozen-native-smoke.log`、
  `target/frozen-release-preflight.log`；元数据为
  `target/frozen-resolution-validation-debug.txt`、`target/frozen-resolution-validation-release.txt`。
  临时原生模块、CLI 入口及规格编辑临时文件均已移除，再执行完整检查。

| 文件 | SHA-256 |
| --- | --- |
| `crates/image/src/preview.rs` | `47ee5f82738cd65f5c23c28f1d8559d5be60f557fa2a1dc2dced593a2ee13abf` |
| `crates/image/tests/preview.rs` | `663babaddd96d81b182206c1c5fcd0297889f5e265eec03dbcc47995a4ba6fd0` |
| `crates/core/src/frame.rs` | `d1093ccd1b2bf1af9a33cbff869284a0b5cf5a2d6b38d48829eae0673bf01946` |

### 行为结果

| ID | 步骤 | 预期/实际 | 结果 |
| --- | --- | --- | --- |
| IMAGE-LIMIT-02 | 对 2940×1912 合成底图生成冻结呈现，检查奇数坐标细线和末尾像素 | 修复前垂直白线变黑、测试失败；修复后水平/垂直细线、边缘标记及原始尺寸保留 | Pass |
| IMAGE-LIMIT-01 | 有行 padding 的通道夹具生成冻结呈现 | 有效像素与通道保持，padding 被移除，原始底图未改变 | Pass |
| MAC-FROZEN-PIXELS-20-01 | 合成原尺寸帧经真实 coordinator 创建冻结窗口 | 输入/RenderImage 均 2940×1912；viewport=1470×956、scale=2，物理像素映射为 1:1 | Pass |
| MAC-FROZEN-CLEANUP-20-01 | 上述真实窗口接收 Esc，经 coordinator tick 后等待 500 ms | Resident、frame=None；GPUI 窗口=0，RenderImage 弱引用不可升级 | Pass |
| PREVIEW-QUALITY-20-02 | 新 Release 的真实捕获与清晰度人工复测 | 身份隔离后，用户确认权限提示可用、冻结清晰度接近原屏及 Esc 均符合预期；运行日志的原始尺寸为 2940×1912 | Pass |

完整 `scripts/check-all.sh` exit 0，27 个 Rust 集成测试通过；fmt、Clippy、doc、
验证/依赖/架构/文字检查、Debug/Release 启动/菜单/窗口枚举与驻留无网络检查通过。
合成原生冒烟输出为：

```text
frozen_native_smoke=pass source=2940x1912 render=2940x1912 viewport=Size { 1470px × 956px } scale=2 physical_pixel_mapping=1:1 no_capture=true
frozen_native_cleanup=pass escape=true windows=0 render_image_released=true
```

修复后 Release CDHash 为 `e389e874c560306120b1a644474dd114e99d9d0b`，与之前已授权的
`c38000f98cad87a05a29db0b057f3b2054070401` 不同。旧 Release 在替换 bundle 前已退出；
后续固定本次 Release 复测，不再混用 Debug。原始清晰度 Fail 保留为历史结果，
不因像素/窗口自动化通过而冒称真人观感、完整色彩矩阵、性能或压力验收通过。

复测会话已明确启动，核实仅 PID 43530 运行上述新 Release 路径。后续日志为
`target/frozen-manual-release.log`、`target/frozen-manual-release-error.log`；已在 Finder
选中对应 Release bundle，等待用户为此新签名恢复屏幕录制授权并复测清晰度。

## Debug / Release 身份隔离修复

用户随后反馈按步骤重新授权仍不可用；没有将此反馈视为误操作，也没有重置权限。
日志显示本地 21:31:34 删除旧 ScreenCapture 条目后，授权校验要求已更新为新 Release
哈希 `e389e874c560306120b1a644474dd114e99d9d0b`，但失败时实际进程又变为 PID 44295、
Debug 路径。21:32:15.121 的 TCC 拒绝记录为：

```text
Failed to match existing code requirement for subject dev.arcshot.app and service kTCCServiceScreenCapture
    cdhash H"e389e874c560306120b1a644474dd114e99d9d0b"
    cdhash H"d7b55d314cbfa33201cf423b9de2bac23cb64c50"
```

已确认授权匹配新 Release、运行实例为不匹配的 Debug。[INFERENCE] 系统重开按共享
bundle ID 解析到 Debug，是实例再次错位的原因；只提醒固定路径不能消除这个条件。

修复构建脚本：Release 保留 `dev.arcshot.app` / ArcShot；Debug 在签名前改为独立的
`dev.arcshot.app.debug` / ArcShot Debug，并设置对应 CFBundleDisplayName。偏好与 TCC
权限也由各自 ID 分离。停止错误 Debug 进程后，仅注销旧 Debug 路径的 LaunchServices
注册，再注册两个不同身份；没有重置 LaunchServices 数据库或任何应用权限。

### 本轮验证

- UTC：2026-10-30T13:40:17Z；基线 SHA 仍为
  `d9564d875a6bc86f8e303099e51c0561f7e0080f`，`worktree_dirty=true`，修复未提交。
- 构建配置：Debug 重新打包并运行启动回归；Release 保留既有二进制、bundle 与签名，
  经 LaunchServices 按 ID 启动。没有重新构建 Release 或改动 Rust 产品源码。
- 环境：Mac14,2 arm64，macOS 27.2 (26B123)，1470×956@2x；Rust/Cargo 1.99.1，
  Xcode 27.0 (27A266a)，SDK 27.0 (26A425)，Swift 6.4
  (`swiftlang-6.4.0.34.1 clang-2100.3.34.1`)，Metal 32023.921 (`metalfe-32023.921.6`)。
- 夹具：两份真实 bundle、系统 NSWorkspace 身份解析、实际菜单启动/退出与无捕获预检。
- 日志：`target/bundle-identity-debug-smoke.log`、`target/bundle-identity-launch-lookup.log`、
  `target/identity-debug-launch-smoke.log`、`target/identity-release-launch-smoke.log`、
  `target/identity-release-preflight.log`；环境为 `target/bundle-identity-validation.txt`。
- 本轮新增实现仅为构建/检查脚本及对应文档；Rust 自动化沿用此前完整入口的证据，
  未声称再次运行完整 Rust 套件。实际执行 `bash -n scripts/build-app.sh scripts/check-startup.sh`，
  Debug 启动回归以及两份 bundle 的 LaunchServices 启动和 Release 权限预检。

| ID | 步骤 | 修复前/后实际 | 结果 |
| --- | --- | --- | --- |
| MAC-BUNDLE-ID-20-01 | `scripts/check-startup.sh` 先检查构建身份，再运行真实启动 | 修复前因 dev.arcshot.app / ArcShot 失败；修复后 Debug 为 dev.arcshot.app.debug / ArcShot Debug，菜单/窗口枚举/退出通过 | Pass |
| MAC-LAUNCH-ID-20-01 | NSWorkspace 按两个 bundle ID 查询应用 URL | dev.arcshot.app 精确解析到 Release；dev.arcshot.app.debug 精确解析到 Debug | Pass |
| MAC-LAUNCH-ID-20-02 | `open -n -b <ID> --args --smoke-test` 分别启动两份已注册 bundle | 两个 ID 均完成真实状态项、菜单、窗口枚举、正常退出；无屏幕捕获 | Pass |
| MAC-PERMISSION-MANUAL-20-02（隔离后原生恢复子集） | 按 Release ID 执行无捕获预检并再次核对签名 | screen_recording=true guide_seen=true；Release CDHash 仍为 e389e874c560306120b1a644474dd114e99d9d0b，未再次请求/重置授权 | Pass |

本轮脚本 SHA-256：

| 文件 | SHA-256 |
| --- | --- |
| `scripts/build-app.sh` | `e8c571bea8e1815e2a7baba40a94e1d6589012d839f2add1e81239e45cf8123a` |
| `scripts/check-startup.sh` | `7de16b26da9fb4378ac5f96f9bf28a6937c3919bc3592339527668b7d8edac2f` |

最后明确按 Release ID 启动 GUI，核实仅 PID 45295 运行 Release 路径。复测日志为
`target/identity-manual-release.log`、`target/identity-manual-release-error.log`。
用户随后对 GUI 权限可用、原尺寸冻结清晰度及 Esc 回复「全部符合预期」，据此将
PREVIEW-QUALITY-20-02 更新为通过；该结论仅为用户观感，不替代独立像素/色彩 oracle。
同一构建配置后续修改二进制仍会改变 ad-hoc CDHash，身份隔离不替代稳定签名或重新授权。

此次真实捕获仍为 PID 45295 的 Release，`target/identity-manual-release-error.log`
记录 `capture_request_us=129327 display=1 pixels=2940x1912 stride=11760 scale=2`，
`capture_wake_window_submit_us=337938 pixels=[2940, 1912]`。这是单次样本与窗口提交
时间，不判定性能门槛通过。显示失效及真实原生故障矩阵仍需独立验证。

### 动态背景冻结人工验证

| ID | 用户步骤 | 用户反馈与实际 | 结果 |
| --- | --- | --- | --- |
| MAC-REGION-DYNAMIC-20-01 | 系统时钟秒表持续计时 → 开始截图 → 冻结中等待约 5 秒 → Esc 返回 | 用户确认全部符合预期：冻结数字保持不变，Esc 后真实秒表时间已前进且继续计时 | Pass |

本例验证独立应用持续变化的背景不会更新冻结底图；不扩展为任意动态内容的滚动
兼容或性能结论。随后日志仍为完整 2940×1912、stride=11760、scale=2 捕获。

冻结期间从菜单再次开始的真人步骤暂不安排：锁定 GPUI 后端的 PopUp 窗口层级为
101，当前全屏覆盖层高于菜单栏；全局快捷键入口属于尚未完成的 #21。核心重启动作
的既有自动化证据保留，不冒充当前真人入口已通过。

### 权限撤销、恢复及系统重开人工验证

| ID | 用户步骤 | 用户反馈与实际 | 结果 |
| --- | --- | --- | --- |
| MAC-PERMISSION-RECOVERY-20-03 | 驻留时关闭 ArcShot 的屏幕录制开关 → 按系统提示退出重开 → 开始截图并取消不可用提示 → 恢复同一条目权限 → 按提示重开/重检 → 截图与 Esc | 用户确认全部符合预期：关闭后明确不可用且未冻结，恢复后可用，截图与 Esc 正常；未删除条目或重置全局权限 | Pass |

用户反馈后只读核对实际进程为 PID 48007、Release 路径。这说明恢复后的当前实例
没有再次落到 Debug。用户未单独报告系统是否显示重开提示，不据「全部符合预期」
推断具体提示分支。该用例从驻留状态撤销权限，不冒称冻结中直接撤权路径已验证。

### 显示失效测试暂缓

已编译并以只读 `--plan` 核对系统支持的模式：当前 mode=6、1470×956 逻辑点、
2940×1912 物理像素、2x；候选 mode=4、1280×832、2560×1664、2x。计划采用
10 秒倒计时、8 秒 app-only 临时配置，再显式恢复原模式；该计划不是实际切换证据。
只读输出摘要保存在 `target/display-invalidation-plan.log`。

用户明确选择「暂缓这项测试」，因此未调用任何显示配置更改接口。临时 Swift 工具
及可执行文件清理，不加入产品或仓库常驻测试入口。

| ID | 实际 | 结果 |
| --- | --- | --- |
| MAC-TARGET-MANUAL-20-01 | 用户暂缓；仅只读枚举模式，没有切换、没有触发产品显示失效处理 | Not tested |

原生受控错误/超时、独立颜色/alpha/精确窗口排除 oracle 和上述显示失效仍保留为
验收缺口；不因此关闭 #20 或把未运行项目视为通过。

### 取消截图保留原有剪贴板

| ID | 用户步骤 | 用户反馈与实际 | 结果 |
| --- | --- | --- | --- |
| MAC-CANCEL-CLIPBOARD-20-01 | 在文本编辑复制固定文字 → 开始截图并冻结 → Esc 取消 → 在新行粘贴 | 用户反馈「仍是原文字」，取消后未以图像或空内容覆盖原有剪贴板 | Pass |

此例只覆盖当前无输出切片的取消行为，不替代后续 Finish/保存失败、PNGf 或输出
字节一致性用例，也不把取消通过扩展为已验证所有时刻的剪贴板状态。

### 受控绝对期限故障测试

核对锁定依赖后确认 `SCREENCAPTUREKIT_COMPLETION_TIMEOUT_SECS` 仅接受整秒，`0`
会禁用依赖的超时。因此未用该变量伪造立即超时，也未更改测试进程环境。

临时外部控制器只读验证可检查当前 Release 的线程，并以独立已知命名线程夹具
验证 `arcshot-capture` 识别；夹具已清理。用户确认受控暂停测试后，控制器先后
等待 30 秒及 120 秒，两次都未观察到捕获线程，因此均未发送 SIGSTOP/SIGCONT。
第二次服务 `ArcShotDeadlineProbe2` 完整终态输出为：

```text
native_fault_probe=armed waiting_for_capture_worker_seconds=120
native_fault_probe=not_executed reason=no_capture_worker_observed
```

服务 exit=2 表示未执行故障注入，不是 ArcShot 崩溃，也不是应用超时恢复通过。
不能据此判断用户是否点击过截图或线程识别在真实短任务上是否可靠；停止自动重试，
等待用户描述实际界面。只读计划与就绪摘要为 `target/native-deadline-plan.log`、
`target/native-deadline-armed.log`。

| ID | 实际 | 结果 |
| --- | --- | --- |
| MAC-NATIVE-DEADLINE-20-01 | 第 3 轮已观察到捕获线程并暂停/恢复 ArcShot 6 秒；用户确认首次明确要求重开，再次开始仍拒绝捕获、没有新冻结画面 | Pass |
| MAC-NATIVE-RELAUNCH-20-01 | 同一 Release 新实例 PID 55653 启动后，用户确认权限仍可用、真实捕获正常进入清晰冻结画面、Esc 正常返回驻留 | Pass |

本例只验证应用绝对期限及不确定终态规则，不代替所有真实 SCK 错误、硬件故障或
回调不返回的原生矩阵。

用户随后明确说明前两轮人不在，并请求重新测试；因此此前未触发不能归因为真实
捕获线程识别失败。第 3 轮沿用已获授权的暂停范围，控制器 ArcShotDeadlineProbe3
（PID 55085）已确认 armed，目标仍为 Release PID 48007，等待窗口 120 秒。
第 3 轮控制器随后以 exit=0 正常结束，实际输出为：

```text
native_fault_plan=pass pid=48007 readable_threads=6 capture_worker=absent pause_seconds=6
native_fault_probe=armed waiting_for_capture_worker_seconds=120
native_fault_probe=paused observed_capture_worker=true
native_fault_probe=resumed pause_seconds=6
```

读取完成服务输出后，另只读观察目标 PID 48007 仍在 Release 路径运行，进程状态为 S，
不是暂停态 T。用户随后确认首次出现要求重开的安全提示，第二次开始仍被拒绝；
之后完成下述重开恢复，两个子用例均取得用户反馈。执行摘要为
`target/native-deadline-execution.log`。

为验证恢复，主代理以 SIGTERM 结束故障测试 PID 48007，再按 Release ID 启动同一
未修改签名的 bundle，核实新 PID 55653 为 Release 路径。此处是进程重启，不声称
测试了 AppKit 菜单退出的清理路径。临时控制器源文件及可执行文件已清理，未再次
故障注入；恢复日志为 `target/post-deadline-release.log`、
`target/post-deadline-release-error.log`。用户随后明确反馈「重开后恢复正常」，对应
权限仍可用、正常冻结及 Esc 返回驻留；只读核对当前实例仍为 PID 55653 的 Release。
恢复捕获的实际日志为：

```text
capture_request_us=153469 display=1 pixels=2940x1912 stride=11760 scale=2
capture_wake_window_submit_us=353701 pixels=[2940, 1912]
```

本轮人工测试至此收尾：受控进程暂停后的绝对期限、安全终态、拒绝重试及重开恢复
已有证据；这不是所有真实 SCK 错误或硬件超时通过。显示失效测试按用户选择暂缓，
独立颜色/像素/窗口排除 oracle、性能与压力矩阵仍未完成，#20 保持开放。
暂停及显示切换的临时工具均已清理，当前正常运行的 Release 未启用故障注入。

## 2026-10-31 验收会话：颜色 oracle、显示失效与采样记录

本节为独立运行，不改写旧报告。代理侧准备（提交 `db0f236`–`18a11f0`）：耗时/资源
采样脚本、像素精确夹具、5×5 网格探针；干净 SHA 上 `scripts/check-all.sh` exit 0。

### 实测快照

- 实现提交：探针网格 `a2cb304`，夹具 v3 `18a11f0`；分支 `ticket/20-screen-capture`。
- Release CDHash：`762c17741c04efe7480a008395ca8b1e92b8099e`；GUI 实例路径与 PID
  每步以 `pgrep` 核实。环境沿用本机记录（Mac14,2 arm64，macOS 27.2 (26B123)，
  1470×956@2x，rustc 1.99.1，Xcode 27A266a）。
- 权限：重建后旧条目哈希 `e389e874…` 与新哈希不匹配，仅开关不刷新 TCC 记录的
  designated requirement；用户删除条目并经新弹窗授权后
  `screen_recording=true guide_seen=true`，此后预检多次保持 true。
- 日志：`target/acceptance-preflight*.log`、`target/acceptance-release{,-error}.log`、
  `target/display-invalidation-execution{,2}.log`、`target/resource-observation.log`。

### MAC-PERMISSION-20-REAUTH（新增子用例）

| 步骤 | 实际 | 结果 |
| --- | --- | --- |
| 重建 Release 后仅重新打开既有条目开关 | TCC 日志 `Failed to match existing code requirement`，预检 false；开关不刷新存储哈希 | Fail（预期外行为，流程缺陷而非产品缺陷） |
| 删除条目 → 应用内新弹窗授权 → 重开 | 预检 `screen_recording=true`；后续全部真实捕获成功 | Pass |

### MAC-COLOR-MANUAL-20-01

夹具：`docs/validation/fixtures/color-oracle-20.png` v3，像素精确 2940×1912，四象限
纯 sRGB 红/绿/蓝/白 + 中央洋红标记 + 黑色大字（兼清晰度检查）。Preview 全屏呈现。
探针（`capture_probe_grid`，5×5 内缩网格，行 y=5/25/50/75/95%）实测：

```text
rows=5f3233ff 613233ff 494a33ff 326233ff 326033ff
     | ff0000ff ff0000ff 8fbc17ff 03ff00ff 03ff00ff
     | ff0000ff ff0000ff 8fbc17ff 03ff00ff 03ff00ff
     | 0000ffff 0000ffff 000000ff ffffffff ffffffff
     | 0000ffff 0000ffff aeaeffff ffffffff ffffffff
```

- 方向：红=左上、绿=右上、蓝=左下、白=右下，无上下翻转。Pass。
- 通道：红/蓝无互换，全 alpha=ff。Pass。
- 颜色精确度：红 `ff0000`、蓝 `0000ff`、白 `ffffff` 精确；绿 `03ff00`（R +3/255）。
  以 v2（系统 NSColor）与 v3（显式 sRGB 分量）夹具两次复测同为 +3，判定为内屏
  display-P3 管线经 SCK 回转 sRGB 的单通道舍入，非产品缺陷。公差内 Pass。
- 顶行深灰 = 菜单触发时唤出的系统菜单栏条带（非自有窗口）；中央列混合值位于
  象限分界/文字/标记采样（Preview 按菜单栏下方内容区适配），均不构成 Fail 判据。
- 自有 UI 排除：网格中无准备窗/旧浮层/菜单残留色块。用户确认冻结画面与原屏一致。

结果：**Pass**（用户观察 + 数值 oracle 双证据；不声明显示校准或 GPU 合成色彩管理）。

### MAC-TARGET-MANUAL-20-01（显示失效）

临时工具（已清理）：Swift `CGDisplaySetDisplayMode`，15 秒倒计时 → 切至
2560×1664 保持 8 秒 → 显式恢复；执行与恢复均成功（live mode 复核
1470×956@2940×1912）。

- 第一轮用户未及时进入冻结态，其开始请求在切换窗口内被拒绝：
  `capture_prepare_failure=built-in display unavailable or changed`（拒绝子路径 Pass）。
- 第二轮冻结态下切换：用户确认出现「内建显示器已失效或几何/缩放发生变化，截图
  会话已取消。不会切换到外接屏幕。」提示，无部分画面，恢复原模式后 Esc 返回及
  再次捕获均正常（后续 `capture_request_us=29151` 等成功样本）。

结果：**Pass**（冻结中失效取消 + 明确报错 + 恢复后正常；切换瞬时画面未单独观察，
不构成判据）。失效路径未产生 ERROR 级日志，仅 UI 提示；不影响验收，记录为观察。

### 耗时与资源采集（PERF 记录起点 / RESOURCE 子集）

`scripts/perf-samples.sh` 汇总本会话 10 次真实捕获：

```text
capture_request_us samples=10 median_us=25834 p95_us=189145 max_us=189145
capture_wake_window_submit_us samples=10 median_us=224596 p95_us=445677 max_us=445677
```

`scripts/resource-sample.sh` 在 3 次捕获周期内轮询 41 个样本：
`peak_rss_kb=54528`（≈53 MiB，远低于 2 GiB 准入预算）。

判定：#20 验收条款为“开始记录捕获/唤起耗时”，已满足；PERF-SCK/PERF-REGION 的
预热+30 样本与门槛判定、GPU/显存观测不属于本票，维持 Not tested。RESOURCE-MAX
压力矩阵未执行，本记录仅为常驻+捕获周期 RSS 子集证据。

### 本轮后剩余缺口

- MAC-NATIVE-MANUAL-20-01 的真实 SCK 请求错误子集：仅由上轮受控暂停（绝对期限）
  覆盖，真实回调错误仍 Not tested。
- PERF 门槛判定、GPU/显存与压力矩阵：Not tested（归属性能/资源票）。
- 其余原生人工矩阵（权限、捕获、冻结、Esc、剪贴板、颜色、显示失效、恢复）均已
  取得用户参与证据。
