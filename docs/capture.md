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
- 自有准备窗口先可见并等待 80 ms 注册，主线程列举其精确窗口 ID，在同一个 SCK
  display filter 中排除；任何映射失败不捕获。采样期间隐藏状态项，禁止新建自有
  窗口；成功后才替换准备窗为冻结覆盖层。取消只关闭窗口，不解除原生请求栅栏。
- Screen Recording 的首引导可以稍后或关闭；用户处理后持久记忆。菜单/截图仍按需
  检查真实权限。显式“配置权限/打开系统设置”才请求该权限，返回应用时重检；
  “重新检查”不重发请求。普通截图不检查 AX / Input Monitoring。
- 原生仅走 SCScreenshotManager.capture_image。sourceRect 为 display-local
  (0,0,width,height) 逻辑点，global screen origin 仅用于身份/几何核对；
  输出物理像素由 filter.pointPixelScale=2 决定。
- 显式 sRGB、SDR、BGRA、无光标；通过命名 kCGColorSpaceSRGB 的 8-bit CGContext
  转换后，验证尺寸、stride、字节数与所有 alpha=255，才进入中性 CapturedFrame。
- 分配前限制 24M 像素、边长 16384，native 原始存储 ≤192,000,000 字节。
  中性帧接纳还预留 8 个行跨度表面和 272 MiB 预览/运行时余量，合计 ≤2 GiB；
  这是保守 admission accounting，不是实测 footprint。预览 ≤4M 像素，使用
  image 的 presentation-only 采样，原始 2x 底图不缩小。取消释放底图、预览及
  GPUI image cache 引用；不承诺安全擦除。

## 无捕获预检

```sh
scripts/build-app.sh
target/bundle/debug/ArcShot.app/Contents/MacOS/arcshot --check-capture-environment
```

只报告内屏 ID/逻辑几何/scale、Screen Recording preflight 和首引导偏好。
不请求权限、不读取像素。不要把该预检的成功当成捕获成功。

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
