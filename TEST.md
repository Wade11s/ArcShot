# ArcShot 测试策略

本文是根目录测试入口，依据 #18 的 T-01～T-05 与 #19 已落地检查维护。
目标是从最高行为接口取得可信证据，而不是用私有实现断言、mock 窗口或原型结果
替代正式产品验收。

## 1. 开发循环

- 只在父规格已确认的接缝编写行为测试：core 语义动作、固定图像/文档、app
  副作用与调度、macOS 实机。测试名称使用 `GLOSSARY.md` 的术语。
- 按一个测试 → 一个最小实现的 red/green 垂直切片工作；不在未确认的私有字段、
  helper 或额外通用架构上建测试。
- 开发中经常运行 `cargo check --locked --workspace` 与当前单个测试文件；提交前
  运行一次完整入口 `scripts/check-all.sh`。
- 正常失败以 `Result` 和恢复行为断言，不用 panic 当测试通过。worker panic、原生
  请求状态不确定、过期任务与副作用前版本栅栏在功能首次落地时覆盖。

## 2. 当前完整入口

```sh
scripts/check-all.sh
```

该命令依次运行：

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo doc --locked --workspace --no-deps
scripts/check-architecture.sh
scripts/check-text-backend.sh
scripts/check-startup.sh
scripts/check-no-network.sh
scripts/check-startup.sh --release
```

### 已实现的稳定检查

| 稳定 ID | 接缝与命令 | 必须断言 | 明确不覆盖 |
| --- | --- | --- | --- |
| ARCH-01 | `scripts/check-architecture.sh` | app → core/image/macos；image/macos → core；两者不互依赖；core 无反向产品依赖；GPUI 仅在 app | crate 内模块设计 |
| TEXT-01 | `scripts/check-text-backend.sh` | GPUI `font-kit` 返回真实 Helvetica `i < W` advance，禁止 NoopTextSystem | 中文 IME、emoji、实际标注排版 |
| MAC-START-01 | `scripts/check-startup.sh` | 正式 bundle 进入 run loop；Accessory 无 Dock 策略；真实可见 NSStatusItem、图像与菜单；`terminate:` 后 exit 0；5 秒看门狗 | 完整 MAC-START、Finder 人工视觉/点击 |
| PRIVACY-01 | `scripts/check-no-network.sh` | 正常驻留至少 0.5 秒且 `lsof -i` 无网络/监听 socket；工具缺失/检查异常非零返回，不得假通过 | 后续功能与所有时刻的网络证明 |
| BUILD-RELEASE-01 | `scripts/check-startup.sh --release` | Release bundle 同样完成驻留/退出 | 签名、公证、性能与资源门槛 |

当前实测记录见 `docs/validation/19-startup.md`。这些脚本使用正式产品二进制与
AppKit 对象；原生冒烟通过不等于完整 macOS 发布矩阵通过。

## 3. Rust 自动化接缝（随功能落地）

下表是固定测试组，不是允许用一个含大量断言的大测试代替子用例。除本票已实现的
启动/文字检查外，其余功能尚未实现，状态均为 **Not tested**。

| 稳定用例组 | 公共入口与覆盖 | 必须观察的行为 |
| --- | --- | --- |
| CORE-SESSION | core 语义动作：会话转换、选区、Finish、分层 Esc、拖动、快捷键重启/互斥 | I-05/I-09/I-10/I-11 状态一致；取消/重启清理；无操作无历史 |
| CORE-EDIT | core 文档动作：对象创建/命中/编辑/删除/样式、文字提交与历史 | 几何、内容、层叠、选区、撤销/重做；不要求恢复选中态 |
| CORE-COORD | 屏幕/图像/视图/物理输出转换，2x 与畸形尺寸 | 无重复缩放/取整、越界或溢出；视图不改变完整输出 |
| IMAGE-COMPOSE | 固定图像/文档与明确字体：全部工具、层叠、裁剪、SDR sRGB | 固定参考/像素真值；文字 golden 使用可复现字体，原生字体/IME 另验 |
| IMAGE-BLUR | 三档 Blur、移动/调整、边缘与上下层 | 4/8/12 点语义、无色罩、不采样选区外、后建对象清晰、预览/输出一致 |
| IMAGE-STITCH | 固定帧序列：上下追加、反向、单帧、歧义/变化/缺口 | 仅严格验证后扩展；无重复/缺行/猜补；无法验证即显式失败 |
| IMAGE-LIMIT | 尺寸、总像素、预览、编码与预算输入 | 分配前检查；失败不超预算分配、不生成损坏输出 |
| APP-EFFECT | 注入权限、捕获、合成、编码、剪贴板、保存结果 | 成功/失败/取消/重试/放弃符合 I-04/I-10/I-11；同版本复用 PNG 字节 |
| APP-ASYNC | 可控调度：单在途、2 帧队列、背压、取消/重启、迟到结果、worker 异常 | 不重复任务/输出、不污染新会话；不确定原生状态禁止部分输出与会话内重启 |
| APP-FRAME | 注入宽高/stride/字节数/alpha/溢出与迟到帧 | 接纳前拒绝；保持命名 sRGB、不透明中性帧合同；不污染新文档 |

新增子用例必须记录稳定 ID、前置、步骤、预期与实际，不通过删除规格边界来减少
测试。模拟成功不能证明权限、焦点、IME、原生捕获或真实资源生命周期成功。

## 4. macOS 实机发布矩阵

实机使用正式产品构建，记录环境与人工操作。以下矩阵均为发布前必测；当前只有
MAC-START-01 与 PRIVACY-01 的窄范围自动化通过，其余不得继承原型结论。

| 稳定用例组 | 实机范围 |
| --- | --- |
| MAC-START | 菜单驻留、无 Dock 主窗口、全部菜单命令、默认/自定义热键、冲突恢复、偏好跨重启、真实登录项状态/失败 |
| MAC-PERMISSION | Region Screen Recording 引导；Scroll 三权限预检、设置返回重检、运行中失效；拒启/失效不输出 |
| MAC-REGION | 单内屏框选、移动/调整、重选/撤销、完成/保存/取消、Esc、快捷键重启、2x 几何 |
| MAC-ANNOTATION | 全工具、命中/控制点、层叠/历史、越界、Blur、密度、悬停/焦点与贴边布局 |
| MAC-TEXT | 中文 IME、拉丁文字、emoji、多行；提交/Finish/保存不丢组合文字；空文本与撤销 |
| MAC-SCROLL-POSITIVE | 自有 GPUI 夹具、TextEdit 固定文档、Safari `file://` 静态页分别完成启动、慢速双向、反向去重、完成/取消/输出 |
| MAC-SCROLL-UI | 右/左/选区内预览、目标内容不被浮层污染、多自有窗口全部精确排除 |
| MAC-SCROLL-INPUT | 单帧、Enter、双击首击/完成、单击拦截、预览上滚动、前台/hit-test、Esc/重启/清理 |
| MAC-SCROLL-NEGATIVE | 动态、固定浮层、重复/低纹理、无重叠跳变、横向/对角/嵌套滚动；无法验证即停止 |
| MAC-TARGET-FAIL | 目标移动/关闭/前台变化、几何/尺度变化、遮挡、输入保护/权限失效、内屏失效 |
| MAC-NATIVE-FAIL | 资源上限、SCK 错误/超时、worker 异常；普通失败与禁止输出/重启的不确定终态分离 |
| MAC-COLOR-OUTPUT | 命名 sRGB 全链路、PNGf 剪贴板、内存解码尺寸/像素/颜色、第三方粘贴 smoke、显式保存字节一致 |
| MAC-RECOVERY | 保存取消/失败、复制失败/重试/放弃、3 秒预览、长图整图标注、取消保留旧剪贴板 |
| MAC-LIFETIME-PRIVACY | 不重复输出/污染会话；回调/窗口/tap/图像引用清理；无自动截图落盘、无运行期联网 |

需要用户授权或视觉判断的项目必须由用户参与。权限切换若先触发焦点停止，只能
记录实际时序，不能冒称“运行中直接撤权”已验证。负面用例通过只表示安全处理，不
外推为任意应用、动态内容、显示器或滚动速度兼容。

## 5. 性能与资源门槛

性能使用正式 Release、已授权且无调试器环境；预热一次后至少 30 个样本，保留全部
原始样本、median、nearest-rank p95 与 max。门槛是发布判据，不是运行中强行终止的
定时器。

| 稳定指标 | p95 / max 门槛 |
| --- | --- |
| PERF-INTERACT | p95 ≤ 100 ms |
| PERF-REGION | p95 ≤ 300 ms |
| PERF-SCROLL-START | p95 ≤ 1 秒 |
| PERF-FRAME-PREVIEW | p95 ≤ 200 ms，max ≤ 500 ms |
| PERF-EVENT-PREVIEW | p95 ≤ 400 ms，max ≤ 750 ms |
| PERF-SCK | p95 ≤ 150 ms |
| PERF-STITCH | p95 ≤ 100 ms（1360×1080 接纳帧） |
| PERF-EXPORT-2M | p95 ≤ 2 秒 |
| PERF-EXPORT-6M | p95 ≤ 5 秒 |
| PERF-EXPORT-24M | 连续 5 次，每次 ≤ 20 秒 |
| PERF-CLIPBOARD | p95 ≤ 500 ms |
| RESOURCE-MAX | 最大导出 5 次观测 peak physical footprint ≤ 2 GiB，同时记录 RSS、native/GPU 与编码阶段 |

资源测量与性能测量分开，采样间隔 ≤ 100 ms，并以 macOS footprint 交叉检查。不得
通过降质、缩图、跳过验证、删除用例或自动落盘达标。当前这些项目全部 **Not tested**。

## 6. 证据、隐私与放行

每份自动化报告或实机日志必须附：

- 稳定 ID、前置、步骤、预期、实际及 **Pass / Fail / Not tested**；
- commit SHA、Debug/Release 配置、确切 Rust/Cargo、Swift、SDK、Metal 工具链；
- 机器、macOS build、内屏几何/scale、权限预检、夹具版本与 UTC 时间戳；
- 性能原始样本与统计；资源采样数据及观测峰值口径。

默认证据只保存文本、数值与 hash。除用户显式保存测试生成的 PNG 外，保存取证
图像必须另获明确授权；不得为日志、规格整理或自动化擅自保存用户截图。Safari
测试只打开本地 `file://` 页面，禁止联网。

崩溃、卡死、损坏输出、过期任务输出、意外截图落盘与运行期联网均为发布阻塞。
发布候选的必测项不能是 Fail 或 Not tested；模拟、原型和单机上的一次通过都不能
替代相应正式产品证据。
