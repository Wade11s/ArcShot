# #20 Capture Session：边界与实机操作

本切片从菜单“开始截图”取得真实内屏冻结源，Esc 取消；不提供选区、复制、保存、
热键或滚动功能。自动化不能替代下列原生验收。执行证据见
`docs/validation/20-capture.md`；未执行的原生项目保持 Not tested。

## 状态与生命周期

- core 是唯一会话权威：Resident → CheckingPermission → Preparing → Frozen。
  app 仅执行有 ticket 的任务，提交 worker 和创建覆盖层前各有版本栅栏。
- Preparing 时取消进入 Draining：界面可关闭，但原生未确认结束前不能再发捕获。
  已确认完成的旧帧被丢弃；安全完成后才允许新的权限检查及捕获。
- SCK 错误（包括首次 shareable-content 查询）、5 秒绝对期限或 worker panic
  进入 RelaunchRequired。没有复制/部分输出或会话内重启入口；不确定 worker 不在
  UI 线程等待，迟到结果只释放引用。5 秒不是 PERF-SCK / PERF-REGION 的门槛。
- app 保留 JoinHandle，完成且可非阻塞 join 后才接纳成功或普通失败；不是仅凭
  通道收到消息便允许重启。panic 只在应用 worker 边界防护，普通错误使用 Result。
- 使用固定单内屏的身份、逻辑几何与 2x scale。native 捕获前后、呈现前、Frozen
  驻留期间持续核对；任何失效结束整个会话，不迁移到外屏。
- 自有准备窗口先可见并等待 80 ms 注册，隐藏状态项后，主线程用 AppKit
  `windowNumbersWithOptions(AllSpaces)` 列举真实自有屏幕窗口，在同一个 SCK display
  filter 中精确排除；不把 `NSApplication.windows()` 中的状态栏代理窗口当作
  CGWindowID。任何枚举/映射失败不捕获，并恢复状态项。采样期间禁止新建自有
  窗口；成功后才替换准备窗为冻结覆盖层。取消只关闭窗口，不解除原生请求栅栏。
- Screen Recording 的首引导可以稍后或关闭；用户处理后持久记忆。菜单/截图仍按需
  检查真实权限。显式“配置权限/打开系统设置”才请求该权限，返回应用时重检；
  “重新检查”不重发请求。普通截图不检查 AX / Input Monitoring。
- 权限及错误提示的按钮、Esc 和原生关闭事件携带来源窗口 ID；提示替换或关闭后，
  旧窗口的排队事件不能关闭新提示或执行权限副作用。
- 原生仅走 SCScreenshotManager.capture_image。sourceRect 为 display-local
  (0,0,width,height) 逻辑点，global screen origin 仅用于身份/几何核对；
  输出物理像素由 filter.pointPixelScale=2 决定。
- 显式 sRGB、SDR、BGRA、无光标；通过命名 kCGColorSpaceSRGB 的 8-bit CGContext
  转换后，验证尺寸、stride、字节数与所有 alpha=255，才进入中性 CapturedFrame。
- 分配前限制 24M 像素、边长 16384，native 原始存储 ≤192,000,000 字节。
  中性帧接纳还预留 8 个行跨度表面和 272 MiB 缩略预览/运行时余量，合计 ≤2 GiB；
  这是保守 admission accounting，不是实测 footprint。普通冻结画面以原始物理
  分辨率呈现，image 只移除行 padding，不降采样；该表面及 GPUI 上传/纹理包含
  在全尺寸表面预算中。4M 上限仅用于缩略预览（含滚动缩略预览）。取消释放底图、
  冻结表面及 GPUI image cache 引用；不承诺安全擦除。

## 无捕获预检

```sh
scripts/build-app.sh
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot --check-capture-environment
```

只报告内屏 ID/逻辑几何/scale、Screen Recording preflight 和首引导偏好。
不请求权限、不读取像素。不要把该预检的成功当成捕获成功。

### 固定授权对象与启动方式

实机测试先选定 Debug 或 Release，授权后保持同一个 bundle 和签名。Release 使用
`dev.arcshot.app` / ArcShot，Debug 使用 `dev.arcshot.app.debug` / ArcShot Debug，
授权与偏好相互独立。旧 Debug 曾与 Release 共用 ID；本次已注销旧 Debug 的系统
启动注册，并按新身份重新注册，Release 保持原签名与授权。
同一配置重新编译仍可能改变 ad-hoc designated requirement 中的 CDHash。若 TCC
日志出现 `Failed to match existing code requirement`，先核对实际路径、ID 和签名，
不要直接重置全局权限。

测试 Release 时先退出已有 ArcShot，再明确新建实例并核实进程路径：

```sh
open -n target/bundle/release/ArcShot.app
pgrep -fl 'ArcShot.app/Contents/MacOS/arcshot'
```

后者必须只显示目标 Release 路径；只看到菜单栏图标不能证明运行的是哪个构建。
从 agent/终端直接调用可执行文件时，TCC 可能把权限归属到启动它的终端应用；
需要证明 GUI 授权时，通过 LaunchServices 运行无捕获预检并查看其输出日志：

```sh
open -n --stdout "$PWD/target/capture-preflight.log" \
  target/bundle/release/ArcShot.app --args --check-capture-environment
```

等待该短暂进程退出后读取 `target/capture-preflight.log`；该入口不会显示 GUI、
请求权限或捕获像素。后续 GUI 测试仍使用上述明确启动的同一个 Release bundle。

## 需用户参与的稳定子用例

使用用户授权的正式 bundle；优先 Release。除非另获明确授权，不保存任何取证
截图。文本运行日志可以保存在 target/。授权是 macOS 的用户操作，代理不能代点。
使用当前偏好记录真实前置；不要为重演首次启动而擅自清除用户偏好。

| ID | 前置 | 步骤 | 必须观察的预期 |
| --- | --- | --- | --- |
| MAC-PERMISSION-MANUAL-20-01 | 尚未处理首引导 | 启动 → 稍后再说 → 退出 → 重启 | 首次有配置/稍后；重启不重复；仍能用菜单 |
| MAC-PERMISSION-MANUAL-20-02 | Screen Recording 不可用 | 开始截图/权限菜单 → 打开设置 → 用户操作 → 返回应用 → 重检/取消 | 显示“屏幕录制权限尚不可用”，提供设置/重检/取消；不猜拒绝原因；必要时说明重启；不启动冻结会话；不请求额外权限 |
| MAC-REGION-MANUAL-20-01 | 已授权；仅内屏；受控静态内容 | 菜单开始截图；背景随后变化；检查四角、文字方向与颜色 | 冻结源为原屏，2940×1912 物理像素（当前 1470×956@2x）；没有自有准备窗/旧浮层或菜单；显示保持冻结 |
| MAC-REGION-MANUAL-20-02 | 已冻结 | Esc；再次开始；再 Esc | 返回菜单栏，无窗口、复制、保存或迟到重现；新会话无旧帧 |
| MAC-TARGET-MANUAL-20-01 | 已冻结 | 用户改变内屏几何/拓扑 | 取消、明确报错；不迁移显示器、不提供部分结果 |
| MAC-NATIVE-MANUAL-20-01 | 授权的受控故障环境 | 原生请求错误/超时；观察取消和再次开始 | 清理可清理资源；提示重开；禁止输出/部分/会话内重启。注入单元测试不替代该原生项目 |
| MAC-COLOR-MANUAL-20-01 | 已授权；可独立验证的固定颜色/方向夹具 | 全链路捕获及冻结，核对角点/命名 sRGB/opaque 数值 | 无上下翻转/通道错误；原生色彩、精确排除需产品证据，不能引用原型通过 |

每行须填写实际及 Pass/Fail/Not tested，附实测 SHA、配置、UTC、工具链、
macOS build、显示几何/scale、权限和夹具版本。捕获证据只允许文本/数值/hash；
取证图像需用户另行授权。

## 耗时记录口径

stderr 的文本诊断包含：
- `capture_request_us`：仅 SCScreenshotManager.capture_image 调用，单位微秒。
- `capture_wake_window_submit_us`：从 app 消费开始菜单命令到创建冻结窗口，
  包含准备、原生、归一化和后台预览；不等于首帧已呈现或指针输入的端到端耗时。

当前只是采集起点，不声明性能通过。PERF-REGION 仍需单独的实际首次呈现计时，
Release、预热一次及至少 30 个原始样本，按 TEST.md 报告 median/p95/max。

### 采样与资源观测工具

```sh
scripts/perf-samples.sh target/<manual-log> --min-samples 30
scripts/resource-sample.sh <arcshot-pid> 1 30
```

`perf-samples.sh` 聚合日志中的 `capture_request_us`（PERF-SCK 口径）与
`capture_wake_window_submit_us`（仅上下文），输出 median/p95/max；样本不足
`--min-samples`（默认 30）时 exit 1。预热样本按 TEST.md 口径在采集时丢弃，
脚本不自行判定性能达标。`resource-sample.sh` 以 ps 轮询驻留进程 RSS 并
报告峰值，只输出文本数值，不读取屏幕像素。

## 颜色/方向 oracle（MAC-COLOR-MANUAL-20-01）

接纳成功后 stderr 会输出一行文本探针：

```text
capture_probe_rgb_hex tl=… tr=… bl=… br= center=…
```

值为全屏夹具四角与中心物理像素的 RRGGBBAA（BGRA 存储换算，忽略行 padding，
alpha 恒为 ff）。实机步骤：全屏显示已知角点颜色的夹具（如四象限纯色
红/绿/蓝/白），菜单开始截图，从日志读取探针值并与夹具定义比对——同时验证
无上下翻转（tl/tr 与 bl/br 位置对应）、无通道交换（R/B 不互串）与不透明。
仅记录数值文本，不保存取证图像。
