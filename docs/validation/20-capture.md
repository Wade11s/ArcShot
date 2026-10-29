# #20 权限与冻结 Capture Session 验证记录

本记录不是产品原生验收通过结论。权限、真实捕获与冻结/取消的真人矩阵仍为
**Not tested**，#20 保持开放，PR 不自动合并。没有捕获或保存用户屏幕图像。

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
