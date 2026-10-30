# ArcShot 测试策略

本文是根目录测试入口，依据 #18 的 T-01～T-05 与 #19/#20 已落地检查维护。
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
scripts/test-validation-tools.sh
scripts/check-validation.sh
scripts/test-dependency-inspection.py
scripts/inspect-dependencies.py
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
| EVIDENCE-01 | `scripts/test-validation-tools.sh` 与 `scripts/check-validation.sh` | 元数据记录字段完整；合法报告通过；缺失/空字段、占位、非法配置、工具或 Git 失败均失败 | 证据内容本身是否正确 |
| DEPENDENCY-01 | `scripts/test-dependency-inspection.py` 与 `scripts/inspect-dependencies.py` | normal/build/dev 直接依赖、来源与重复版本均通过确定性 fixtures；异常来源/畸形/缺失 metadata 失败；实际输出有界 | 许可证、漏洞与所有传递非 Rust 风险 |
| TEXT-01 | `scripts/check-text-backend.sh` | GPUI `font-kit` 返回真实 Helvetica `i < W` advance，禁止 NoopTextSystem | 中文 IME、emoji、实际标注排版 |
| MAC-START-01 | `scripts/check-startup.sh` | 正式 bundle 进入 run loop；Accessory 无 Dock 策略；真实可见 NSStatusItem、图像与菜单；`terminate:` 后 exit 0；5 秒看门狗 | 完整 MAC-START、Finder 人工视觉/点击 |
| PRIVACY-01 | `scripts/check-no-network.sh` | 正常驻留至少 0.5 秒且 `lsof -i` 无网络/监听 socket；工具缺失/检查异常非零返回，不得假通过 | 后续功能与所有时刻的网络证明 |
| BUILD-RELEASE-01 | `scripts/check-startup.sh --release` | Release bundle 同样完成驻留/退出 | 签名、公证、性能与资源门槛 |

当前实测记录见 `docs/validation/19-startup.md`。这些脚本使用正式产品二进制与
AppKit 对象；原生冒烟通过不等于完整 macOS 发布矩阵通过。

## 3. Rust 自动化接缝（随功能落地）

下表是固定测试组，不是允许用一个含大量断言的大测试代替子用例。#20 已实现无选区的权限/捕获/冻结/取消子集；#22 已实现拖拽选区、Finish/双击/完成入口、
2x 裁剪、共享合成/编码、PNGf 剪贴板、完成音效与输出栅栏。标注编辑、选区移动/调整/重选、
显式保存、滚动截图与偏好设置仍未实现，状态仍为 **Not tested**。

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

### #20 的稳定自动化入口

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/session.rs` | CORE-SESSION-01～05：权限先行、冻结/Esc、权限不可用、显示失效、普通失败重试；APP-ASYNC-01～04：取消 drain、串行重启、取消后不确定终态、过期 preflight | 真实 TCC/SCK/窗口 |
| `crates/core/tests/stale_input.rs` | APP-ASYNC-08～09：旧覆盖层 Esc 晚于菜单 Start 时，不能取消新会话或抹掉排队重启 | 真实输入排队顺序 |
| `crates/core/tests/frame.rs` | CORE-COORD-01～02：global/display-local 与物理尺寸、畸形几何；APP-FRAME-01～02：行 padding、尺寸/stride/字节数/溢出/alpha/预算 | 颜色来源；native 必须先转换 named sRGB |
| `crates/app/tests/capture_tasks.rs` | APP-ASYNC-05～07：独立绝对期限、worker unwind、单在途与已知结果释放 | 真实原生请求退出确认 |
| `crates/app/tests/permission_flow.rs` | APP-EFFECT-01～03：稍后及重启、设置返回重检、按需配置/取消 | 系统权限提示与真实持久偏好 |
| `crates/image/tests/preview.rs` | IMAGE-LIMIT-01～02：padding/通道、不修改底图、原尺寸保留单物理像素细线和末尾像素 | 最终合成与输出、原生显示观感 |
| `crates/macos/tests/native_preflight.rs` | 失效身份先于 SCK 拒绝、内屏预检、错误分级、AppKit worker 拒绝 | 实际屏幕捕获 |
| `scripts/check-startup.sh` | 新增 MAC-MENU-20-01：真实 NSMenu 三个 action 各入队，未启动 coordinator/SCK | 真人点击和捕获 |
| `scripts/check-startup.sh` | 新增 MAC-HOTKEY-21-01：注册默认 ⌘⇧X、suspend、resume 与清理均成功；无合成按键，不启动会话 | 真实按键事件投递、冲突/恢复的人工复现 |
| `scripts/check-startup.sh` | MAC-WINDOW-20-01：只有真实状态项的进程枚举到零个可捕获自有窗口，不把 macOS 27 状态栏代理编号误当 CGWindowID | SCK 实际窗口匹配/像素排除 |
| `scripts/check-startup.sh` | MAC-BUNDLE-ID-20-01：启动前核对配置对应的 ID/名称，Debug 为 dev.arcshot.app.debug / ArcShot Debug，Release 为 dev.arcshot.app / ArcShot | 系统按 ID 重启的解析另需原生验收 |

例如单文件执行 `cargo test --locked -p arcshot-app --test capture_tasks`。

### #21 的稳定自动化入口

热键与重启切片只覆盖已确认接缝：core 语义动作、app 副作用策略、启动冒烟的原生注册。
真实按键投递、系统冲突、偏好跨重启与设置窗口人工操作属于 MAC-START，仍为用户参与项。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/hotkey.rs` | CORE-HOTKEY-01～04：默认组合、合法性矩阵、显示顺序、存储编解码与畸形值拒绝 | 键盘布局、非 ANSI 键 |
| `crates/core/tests/session.rs` | CORE-SESSION-06：输出/保存对话框互斥期间忽略 Start，释放后恢复；CORE-SESSION-07：热键重启丢弃旧底图、新捕获失败回驻留且不恢复旧会话 | 输出动作本身（后续切片） |
| `crates/app/tests/hotkey_flow.rs` | APP-EFFECT-04～10：启动注册/失败提示、保存成功替换并持久化、保存失败保留旧配置并恢复注册、恢复失败明确报错、暂挂失败退出录入、Esc 取消不保存、未变更保存无操作 | 原生注册结果与真实按键 |
| `crates/app/src/main.rs`（`--smoke-test`） | MAC-HOTKEY-21-01：默认 ⌘⇧X 注册、suspend、resume 与 Drop 清理 | 事件投递、冲突、用户按键 |

热键实测记录见 `docs/validation/21-hotkey.md`。这不是 MAC-START 其余必测项的替代。

### #22 的稳定自动化入口

区域选择、Finish 与复制只覆盖已确认接缝：core 语义/坐标、固定图像、app 的
在途编码边界与真实 AppKit PNGf 冒烟。真实鼠标拖拽、系统剪贴板粘贴、音效与
原生失败注入属于 MAC-REGION / MAC-COLOR-OUTPUT / MAC-RECOVERY，仍为用户参与项。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/document.rs` | CORE-COORD-03～07：逻辑拖动归一化、2x 单次换算、零尺寸/畸形/越界；CORE-DOCUMENT-01 版本；IMAGE-LIMIT-03 物理边/像素上限 | 真实指针事件、原生显示缩放 |
| `crates/core/tests/output.rs` | CORE-SESSION-08～13：零选区不完成、Finish job、双击命中、输出期互斥/取消；APP-EFFECT-12～14：成功结束、失败保留文档、过期版本/迟到结果拒绝 | 剪贴板与编码副作用本身 |
| `crates/image/tests/compose.rs` | IMAGE-COMPOSE-01～03：物理裁剪/通道/padding、越界拒绝、内存 PNG 解码尺寸/像素/sRGB chunk；IMAGE-LIMIT-04 合成预算 | 原生字体/标注合成、全屏大图压力 |
| `crates/app/tests/output_tasks.rs` | APP-EFFECT-15～17：注入编码结果、worker panic 可重试、单在途 | 真实 24M 导出耗时与内存峰值 |
| `crates/app/tests/key_policy.rs` | APP-KEY-01～03：拖动中忽略 Esc/Enter、输出中忽略 Enter 但 Esc 可取消 | 文字编辑/工具激活时的完整 I-09 优先级 |
| `scripts/check-startup.sh` | MAC-COLOR-OUTPUT-22-01：真实进程内编码 PNG、私有命名 pasteboard PNGf 写入/回读、AppKit 内存解码尺寸；脚本同时断言该行 | 用户剪贴板、非 ArcShot 应用粘贴、颜色真值 |
| `crates/macos/tests/native_preflight.rs` | MAC-COLOR-OUTPUT-22-02：worker 线程访问剪贴板/音效时在 AppKit 副作用前拒绝 | 主线程真实写入与播放已由启动冒烟/实机覆盖 |

实机步骤与预算口径见 `docs/output.md`；实测记录见
`docs/validation/22-region-output.md`。这不是 MAC-REGION 其余必测项的替代。

实机步骤与安全/预算口径见 `docs/capture.md`；实测记录见
`docs/validation/20-capture.md`。这不是 V1 其他子用例的替代。

权限提示替换竞态的原生冒烟记录见上述验证报告：真实 GPUI Esc 先入队、替换提示后
消费，旧 Esc 不关闭新提示，当前 Esc 正常关闭；该场景未纳入常驻自动化入口。

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

CI 的 source/native 分工和 runner 前置条件见 `docs/agents/ci.md`；source 通过
不代表需要 macOS 27 的 Rust/原生检查已经运行。

记录一次实际验证的环境（日志必须已经存在）：

```sh
scripts/record-validation.sh \
  --configuration release \
  --logs target/startup-smoke-release.log,target/resident-smoke.log \
  > target/validation-release.txt
scripts/check-validation.sh
```

将输出附在对应报告中。`worktree_dirty=true` 时必须同时说明未提交差异；不能
用事后另一个 SHA 的环境输出倒填旧测试。报告要求完整 40 位实测实现/脚本 SHA，
`TBD`、`PENDING` 和待填占位符会使检查失败。

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
