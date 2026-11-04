# ArcShot 测试策略

本文是根目录测试入口，依据 #18 的 T-01～T-05 与 #19/#20 已落地检查维护。
目标是从最高行为接口取得可信证据，而不是用私有实现断言、mock 窗口或原型结果
替代正式产品验收。端到端（§4）由代理通过 `agent-desktop` 驱动，可程序化的菜单
命令、热键与按键不再交给用户执行。

## 1. 开发循环

- 只在父规格已确认的接缝编写行为测试：core 语义动作、固定图像/文档、app
  副作用与调度、macOS 实机。测试名称使用 `GLOSSARY.md` 的术语。
- 按一个测试 → 一个最小实现的 red/green 垂直切片工作；不在未确认的私有字段、
  helper 或额外通用架构上建测试。
- 开发中经常运行 `cargo check --locked --workspace` 与当前单个测试文件；提交前
  运行一次完整入口 `scripts/check-all.sh`。
- 正常失败以 `Result` 和恢复行为断言，不用 panic 当测试通过。worker panic、原生
  请求状态不确定、过期任务与副作用前版本栅栏在功能首次落地时覆盖。
- 端到端不是用户任务：能用 `agent-desktop` 驱动的菜单命令、热键与按键由代理执行，
  用户只保留 §4.4 的授权与判断项。可驱动层与观察方式的合同见 §4.2。

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
cargo fetch --locked
scripts/inspect-dependencies.py
scripts/check-architecture.sh
scripts/check-text-backend.sh
scripts/check-startup.sh
scripts/check-no-network.sh
scripts/check-startup.sh --release
```

端到端（§4）不在这个入口里：它需要真实登录会话、可交互桌面、被测 app 的 TCC
授权，并依赖仓库外的 `agent-desktop`。E2E 作为独立证据入口单独运行、单独记录：

```sh
scripts/e2e-agent-desktop.py            # Debug
scripts/e2e-agent-desktop.py --release  # Release
```

### 已实现的稳定检查

| 稳定 ID | 接缝与命令 | 必须断言 | 明确不覆盖 |
| --- | --- | --- | --- |
| ARCH-01 | `scripts/check-architecture.sh` | app → core/image/macos；image/macos → core；两者不互依赖；core 无反向产品依赖；GPUI 仅在 app | crate 内模块设计 |
| EVIDENCE-01 | `scripts/test-validation-tools.sh` 与 `scripts/check-validation.sh` | 元数据记录字段完整；合法报告通过；缺失/空字段、占位、非法配置、工具或 Git 失败均失败 | 证据内容本身是否正确 |
| DEPENDENCY-01 | `scripts/test-dependency-inspection.py` 与 `scripts/inspect-dependencies.py` | normal/build/dev 直接依赖、来源与重复版本均通过确定性 fixtures；异常来源/畸形/缺失 metadata 失败；实际输出有界 | 许可证、漏洞与所有传递非 Rust 风险 |
| TEXT-01 | `scripts/check-text-backend.sh` | GPUI `font-kit` 返回真实 Helvetica `i < W` advance，禁止 NoopTextSystem | 中文 IME、emoji、实际标注排版 |
| MAC-START-01 | `scripts/check-startup.sh` | 正式 bundle 进入 run loop；Accessory 无 Dock 策略；真实可见 NSStatusItem、图像与含两个偏好项的菜单；两个偏好项的 selector 与真实存储/登录项状态一致；完成音效偏好真实域往返并还原；SMAppService 状态只读；PNGf 剪贴板与显式保存临时文件逐字节回读；`terminate:` 后 exit 0；5 秒看门狗 | 完整 MAC-START、Finder 人工视觉/点击、保存面板显示、真实登录项切换 |
| PRIVACY-01 | `scripts/check-no-network.sh` | 正常驻留至少 0.5 秒且 `lsof -i` 无网络/监听 socket；工具缺失/检查异常非零返回，不得假通过 | 后续功能与所有时刻的网络证明 |
| BUILD-RELEASE-01 | `scripts/check-startup.sh --release` | Release bundle 同样完成驻留/退出 | 签名、公证、性能与资源门槛 |

当前实测记录见 `docs/validation/19-startup.md`。这些脚本使用正式产品二进制与
AppKit 对象；原生冒烟通过不等于完整 macOS 发布矩阵通过。

## 3. Rust 自动化接缝（随功能落地）

下表是固定测试组，不是允许用一个含大量断言的大测试代替子用例。#20 已实现无选区的权限/捕获/冻结/取消子集；#22 已实现拖拽选区、Finish/双击/完成入口、
2x 裁剪、共享合成/编码、PNGf 剪贴板、完成音效与输出栅栏；#23 已实现显式 Save PNG、
保存对话框互斥、取消/失败恢复与同一快照的字节复用；#24 已实现选区移动、八向调整、
重选、共享不可变底图的可撤销历史与八个可见控制点；#25 已实现 Rectangle 标注对象、
左侧 Select/Rectangle 工具坞与悬停选项、对象命中/移动/八控制点翻转/删除/样式历史，
以及预览与最终输出共用的像素合成；#26 已实现 Line 的 Solid/Dash、Arrow 的 Flat/Inward、
两端点编辑与像素合成；#30 已实现“完成音效”偏好（默认开、本地记忆、
关闭后成功静默但失败照常报错）与“登录时启动”（真实 `SMAppService` 登录项、菜单
打开读实际状态、切换失败重读并报错）；#27 已实现 Blur Rectangle 三档模糊与层叠编辑；
#28 已实现 Text 多行编辑、中文 IME 输入、可复现字体栅格化与文字历史。滚动截图与
完整标注/文字产品面（真实指针绘制、对象控制点拖动、原生系统字体/IME 候选框）仍未实现，
状态仍为 **Not tested**。

#32 已实现受保护的单帧滚动完成闭环：准备→输入保护→可信首帧→采集→Enter/双击
完成或 Esc 放弃；多帧拼接、位移校验、反向去重、实时预览与三秒完成预览仍由 #33 拥有，
当前保持 **Not tested**。

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

### #25 的稳定自动化入口

Rectangle 标注只覆盖已确认接缝：core 文档/会话动作、固定文档的像素真值合成、app 键位与
指针路由，以及覆盖层工具坞/选项的真实 AX 激活。真实指针绘制、对象控制点拖动、选中后
删除键与预览观感仍属 MAC-ANNOTATION，保持 **Not tested**：控制点与对象没有 AX 节点，
`--headed` 坐标拖拽不能作为验收证据。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/objects.rs` | CORE-EDIT-13～24：工具内连续创建、选区内起始、越界保留/裁剪、零尺寸不入历史、最上层命中、对象拖动优先于选区、八控制点单边调整与越边翻转、空点取消对象选择但拖动仍移动选区、删除、样式选项与工具记忆、每会话默认重置、整次对象拖动一步、无变化不入历史、切工具/取消选择不入历史 | 真实指针、视觉控制点命中区域 |
| `crates/core/tests/session_edit.rs` | CORE-SESSION-24：工具切换、PointerPressed/Moved/Released、样式、删除、撤销/重做共用同一文档与输出互斥栅栏 | 原生事件投递 |
| `crates/image/tests/annotations.rs` | IMAGE-COMPOSE-05～11：默认红/3 点/无填充、2/3/5 点物理线宽、固定色板、20% 同色填充、选区边缘裁剪、创建顺序叠放、任意 crop 与完整合成一致、GPUI BGRA 预览字节 | 原生显示观感、大图压力 |
| `crates/app/tests/key_policy.rs` | APP-KEY-09～12：Esc 分层（对象→工具→会话）、拖动/输出期间忽略、Delete/Backspace 仅在有选中对象时生效、标注工具内双击不完成、对象命中优先于选区手势与双击 | 真实焦点与 IME |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-13～14：冻结覆盖层工具坞可寻址、AX 切换工具并观察 checked/toggled、悬停浮现选项、AX 改线宽与颜色且版本不变；MAC-E2E-12 仍由 AX 取消 | 选区绘制、对象创建/拖动/删除、预览像素 |

`docs/validation/25-rectangle-annotation.md` 记录实测。窗口内工具坞选项依赖悬停（`hover --headed`
仅作为前置，激活证据是 AX 与产品日志）；绘制矩形仍需要坐标拖拽，因此不进入 E2E 通过范围。

### #26 的稳定自动化入口

Line / Arrow 只覆盖已确认接缝：core 的创建/命中/端点/样式语义，固定文档的像素真值合成，
app 键位路由，以及覆盖层四个工具与各自悬停选项的真实 AX 激活。真实指针绘制、端点拖动、
对象命中与虚线/箭头观感仍属 MAC-ANNOTATION，保持 **Not tested**：对象与端点没有 AX 节点，
`--headed` 坐标拖拽不能作为验收证据。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/lines.rs` | CORE-EDIT-25～31：Line/Arrow 工具创建与方向、零长度新建拒绝、水平/垂直线有效、最上层命中与 2 点细线容差、端点改长度/方向且可重合不删除、样式矩阵（同类型才改对象）、按下时样式快照、每会话默认、选区移动携带端点/调整不缩放 | 真实指针、端点控制点命中区域 |
| `crates/core/tests/session_edit.rs` | CORE-SESSION-26：LineOption/ArrowOption 经权威会话修改选中同类对象与工具记忆、输出在途时忽略、版本栅栏一致 | 原生事件投递 |
| `crates/image/tests/lines.rs` | IMAGE-COMPOSE-12～18：2/3/5 点实线物理线宽、2/3/5 点虚线间距随线宽缩放、Flat 等粗线身/平尾/实心三角头、Inward 尖细尾渐宽/凹后缘（三带横截面）且每种线宽都可区分、裁剪、任意 crop 一致、固定色板 | 原生显示观感、大图压力 |
| `crates/app/tests/key_policy.rs` | APP-KEY-13～15：Line/Arrow 工具内双击不完成、区内创建/区外重选、对象命中仍创建、Esc 分层与输出期取消、选择工具下对象命中优先 | 真实焦点与 IME |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-18～21：四个工具可寻址且 checked 互换；悬停 Line/Arrow 出现各自选项；AX 改虚线/线宽/颜色与内凹箭头后 `capture_edit` 诊断与 checked 更新；切回 Rectangle 仍保留自己的样式记忆 | 选区绘制、对象创建/端点拖动、预览像素 |

`docs/validation/26-line-arrow.md` 记录实测。与 #25 一样，窗口内选项依赖悬停作为浮现前置，
激活证据是 AX 行与产品诊断行；对象绘制与端点拖拽仍不进入 E2E 通过范围。

### #27 的稳定自动化入口

Blur Rectangle 只覆盖已确认接缝：core 的创建/编辑/样式/历史语义，固定文档的物理像素
真值高斯模糊，app 的指针/Esc/双击优先级，以及覆盖层第五个工具与三档选项的真实 AX
激活。真实指针绘制、模糊对象命中/控制点拖动、预览观感与 tooltip 文本仍属
MAC-ANNOTATION，保持 **Not tested**：对象没有 AX 节点，`agent-desktop 1.1.0` 也读不到
GPUI tooltip/`aria_description`，`--headed` 坐标拖拽不能作为验收证据。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/blur.rs` | CORE-EDIT-32～34：选区内起始、默认中档、零尺寸拒绝、最上层命中、对象移动/八控制点/删除/选区移动与调整、样式选项只改同类且每会话记忆、按下时样式快照 | 真实指针、控制点命中区域 |
| `crates/core/tests/session_edit.rs` | CORE-SESSION-27：BlurOption 经权威会话修改选中模糊对象与工具记忆、其他工具选项不改、输出在途时忽略、过期 ticket 不改新会话 | 原生事件投递 |
| `crates/image/tests/blur.rs` | IMAGE-BLUR-19～25：4/8/12 逻辑点半径按 2x 转为 8/16/24 物理像素支撑、均匀面无色罩/alpha 变化、模糊只在矩形内、选区边缘像素延伸、下层模糊后层清晰、移动/调整重算、确定性、预览与 PNG 字节一致 | 原生显示观感、大图压力、真实截图内容 |
| `crates/app/tests/key_policy.rs` | APP-KEY-16：Blur 工具内双击不完成、区内创建/区外重选、对象命中仍创建、Esc 分层与输出期取消、Delete 需选中对象 | 真实焦点与 IME |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-23：第五个工具可寻址且 checked 互换；悬停出现三档选项；默认中档；AX 改弱/强后 `capture_edit action=BlurOption … level=…` 诊断与 checked 更新 | 选区绘制、模糊对象创建/拖动、预览像素、tooltip 文本 |

`docs/validation/27-blur.md` 记录实测。与 #25/#26 一样，窗口内选项依赖悬停作为浮现前置，
激活证据是 AX 行与产品诊断行；模糊对象绘制与拖拽仍不进入 E2E 通过范围。tooltip 文案
是真实 GPUI tooltip 与 `aria_description`，并由 `ui::blur_tooltip_tests` 守护；
`agent-desktop 1.1.0` 的 snapshot/find 不暴露该文本，因此 E2E 不断言它。

### #28 的稳定自动化入口

Text 只覆盖已确认接缝：core 的创建/重编/删除/字号取整/历史，固定文档的可复现字体
栅格化，app 的编辑器/IME 键优先级与 Esc 分层，以及覆盖层第六个工具、色板与字号步进
的真实 AX 激活。真实指针绘制、原生系统字体/IME 候选框/焦点、直接输入字号与预览观感仍属
MAC-TEXT，保持 **Not tested**：系统字体适配与 IME 候选窗位置需原生验证，`--headed`
坐标拖拽不能作为验收证据。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/text.rs` | CORE-EDIT-35～39：创建一步、空文本丢弃、重编一步、编辑变空删除且可撤销、字号 6–64/0.5 取整、命中/移动/层叠/工具切换提交；CORE-SESSION-44～45：TextBegin/TextEdited/TextCommit/TextOption 共用编辑栅栏、Finish 先提交、输出在途忽略、TextCancel 丢弃、过期 ticket 不改新会话 | 真实指针、IME 组合文字 |
| `crates/image/tests/text.rs` | IMAGE-COMPOSE-26～34：可复现 5x7 字形的固定 golden、字号/颜色整对象生效、选区裁剪、文字位于模糊层之上、多行与未知字形确定、可插拔字体输入、预览与 PNG 字节一致、逻辑度量与物理位图一致 | 原生系统字体、中文/emoji 真实字形、大图压力 |
| `crates/app/tests/key_policy.rs` | APP-KEY-17：编辑期间 Enter/Delete/Backspace/撤销/重做归编辑器与 IME，Esc 先结束编辑保留内容，拖动/输出期优先，双击文字对象重编 | 真实焦点与 IME 候选框 |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-25：第六个工具可寻址且 checked 互换；悬停出现色板与字号步进；默认红 16；AX 步进写 `capture_edit action=TextOption … size=16.5`、色板写 `stroke=#007AFF`；创建后文字输入可寻址，点击别处提交为一次创建 | 选区绘制、真实文字创建/拖动、直接输入字号、原生字体/IME |

`docs/validation/28-text-ime.md` 记录实测。创建前置使用 `--headed` 拖拽/点击（选区与
对象没有 AX 节点）；验收证据是 AX 激活行与产品诊断行。字号直接输入由 core 的
`TextStyle::quantize_size` 与 UI 的草稿键处理实现，自动化入口为步进按钮与固定取整测试。

### #24 的稳定自动化入口

选区移动/八向调整、重选与历史只覆盖 core 语义、文档/历史合同、app 输入路由与共享裁剪
换算。真实指针拖动、控制点命中、`Cmd+Z` 与覆盖层视觉属于 MAC-REGION，仍为 **Not tested**。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/document.rs` | CORE-COORD-08～09：移动/调整后仍走唯一 2x 裁剪换算；八控制点逻辑命中与极小选区的移动区 | 真实指针坐标、显示缩放 |
| `crates/core/tests/editor.rs` | CORE-EDIT-01～12：整次拖动一步、无变化不入历史、移动携带标注、调整不改标注、八向边缘/最小尺寸、重选替换并可撤销、越界标注保留/裁剪/重现、层叠顺序、同矩形重选不入历史、历史共享底图分配、屏幕边缘舍入余量不反转夹取 | 对象工具、样式、文字、渲染 |
| `crates/core/tests/session_edit.rs` | CORE-SESSION-14～19：拖动实时预览与单步历史、重选、新会话清空历史、输出互斥与撤销后的版本栅栏、过期 ticket、空历史 | 真实按键/鼠标投递、剪贴板 |
| `crates/app/tests/key_policy.rs` | APP-KEY-01～08：拖动中忽略 Esc/Enter/撤销、输出期忽略 Enter/撤销但 Esc 可取消、`Cmd+Z` / `Cmd+Shift+Z` 映射、按下位置的移动/调整/重选路由、双击控制点优先调整 | 真实焦点、键盘布局、事件投递 |
| `crates/image/tests/compose.rs` | IMAGE-COMPOSE-04：移动/调整后的选区经同一裁剪路径得到匹配的物理像素 | 原生显示缩放、大图压力 |

移动/调整后的输出仍复用 `Document::crop` 与 `compose_crop`，没有第二条裁剪路径。实测记录见
`docs/validation/24-selection-history.md`。这不是 MAC-REGION 指针手势或原生通过证据。

实机步骤与预算口径见 `docs/output.md`；实测记录见
`docs/validation/22-region-output.md`。这不是 MAC-REGION 其余必测项的替代。

### #23 的稳定自动化入口

显式保存复用 #22 的统一文档与合成/编码入口。自动化覆盖 core 的保存互斥、取消、
失败、过期结果与目标独立，app 的“只执行一个副作用、同一版本同一字节”以及原生
文件写入回读。真实 `NSSavePanel` 的显示层级、取消与覆盖写属于 MAC-RECOVERY，
仍为实机参与项。#23 合入 #24 时 `CORE-SESSION-14～17` 与选区历史的用例号冲突，
本票改为 `CORE-SESSION-20～23`（14～19 归 #24）。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/save.rs` | CORE-SESSION-20～23：面板期间互斥、取消回编辑、面板失败、无选区/过期 ticket；APP-EFFECT-18～21：选定后合成、成功结束、分类失败保留、过期结果/目标不匹配拒绝、Finish 与 Save 共用快照但互不触发 | 原生面板显示与路径选择 |
| `crates/core/tests/session.rs` | CORE-SESSION-06 改由真实保存对话框驱动：面板打开时忽略菜单/热键 Start，取消后恢复 | 系统热键事件投递 |
| `crates/app/tests/output_publish.rs` | APP-EFFECT-22～26：路径只被其所属 job 使用、版本变化不写旧路径、剪贴板 job 不写文件、同一快照字节一致、超预算在副作用前拒绝 | 真实磁盘与 AppKit 写入 |
| `crates/macos/tests/save_native.rs` | MAC-RECOVERY-23-01：worker 线程创建保存面板、写文件、取消/取结果都在 AppKit 副作用前拒绝 | 主线程真实面板显示 |
| `scripts/check-startup.sh`（`--smoke-test`） | MAC-RECOVERY-23-02：真实进程写临时 PNG 后逐字节读回、拒绝不存在的目标与非 PNG 字节、配置真实 `NSSavePanel`；Debug/Release 各断言 `save_file_smoke=pass` | 面板显示/取消/覆盖写、跨进程文件打开 |

实机步骤见 `docs/output.md`；实测记录见 `docs/validation/23-save-png.md`。

实机步骤与安全/预算口径见 `docs/capture.md`；实测记录见
`docs/validation/20-capture.md`。这不是 V1 其他子用例的替代。

### #30 的稳定自动化入口

两个菜单偏好只覆盖已确认接缝：core 的成功音效开关语义、app 的注入式偏好/登录项
策略、原生的真实偏好域往返与 `SMAppService` 只读状态，以及 E2E 的真实菜单项与
跨重启记忆。真实登录项注册/注销（会改动系统登录项）与失败形态仍属 MAC-START
人工/受控项，不能用 `--headed` 点击或伪造状态替代。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/output.rs` | CORE-SESSION-24～25：默认开；关闭后成功只结束会话、不再发出音效效果；失败仍报告；偏好不被新会话重置 | 真实 `NSSound` 播放与音量 |
| `crates/app/tests/preferences.rs` | APP-EFFECT-27～32：默认开与每次切换持久化；登录项意图必须来自新的实际读取；Applied 后必须再读验证；失败重读并提示、可关闭重试；读取失败不盲改；RequiresApproval 不当作已启用 | 原生 ServiceManagement 注册结果与通知权限 |
| `crates/macos/src/preferences.rs`（单元测试） | MAC-START-30-02：真实偏好 suite 域内“缺失=默认开”、写入/新实例读取/删除回默认 | 用户自身偏好域、跨进程 |
| `scripts/check-startup.sh`（`--smoke-test`） | MAC-START-30-01：五个菜单 action 入队（含 `toggleCompletionSound:`/`toggleLaunchAtLogin:`）；两个偏好项 selector/状态与真实偏好和 `SMAppService` 状态一致；真实偏好域往返并还原；登录项状态只读 | 真实登录项切换、菜单人工点击 |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-01/15/16/17：菜单六项与 selector；完成音效经 AX 切换并写偏好；跨两个真实重启保留并还原偏好；登录项启动/菜单读取一致且从不写 | 登录项注册/注销、失败提示窗口（需真实拒绝） |

实测记录见 `docs/validation/30-sound-login.md`。完成音效的关闭状态只证明
“不产生音效副作用”，真实静音效果与系统音量属于实机观感项。

### #31 的稳定自动化入口

滚动入口、三权限与目标准备只覆盖已确认接缝：core 的入口闸门/三权限矩阵/目标锁定/
自有窗口排除集/失败分级，app 的注入式权限通知策略与有界目标探测，以及原生的三权限
预检、公开 hit-test 目标识别与自有窗口映射。真实 TCC 切换、目标应用前台/指针
hit-test 的实机时序与滚动采样/预览保持 **Not tested**（见 §4.4）。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/scroll.rs` | CORE-SESSION-28～43：无选区拒绝、已有标注拒启且不丢弃、沿用唯一选区进入三权限预检、缺失权限停止并保留区域会话、三权限通过进入目标识别、目标必须完整包含选区、已知目标失败分级、自有窗口映射失败拒绝、成功准备固定排除集、取消保留选区、原生不确定终态禁止重启、内屏失效取消整会话、热键重启放弃准备、过期动作不影响新尝试、三权限用途/键序 | 真实 TCC/SCK/窗口/指针 hit-test |
| `crates/app/tests/scroll_flow.rs` | APP-EFFECT-38～41：缺失权限提供用途/设置/重检/取消；重检不自动启动；取消清除通知与设置等待；Ready 通知不产生启动副作用 | 真实 TCC 切换与系统设置返回时序 |
| `crates/app/tests/scroll_target_tasks.rs` | APP-ASYNC-10～12：绝对期限独立于 SCK；已知目标失败可重试；单在途闸门只在一个已知结果后重开 | 真实 SCK 查询内容与窗口枚举 |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-24：入口在无选区时不可激活；以 `--headed` 拖拽作为前置创建选区；AX 激活入口后出现 `scroll_permission` 诊断；目标识别产生 `scroll_target`/`scroll_refused` 诊断；区域会话与选区保留 | 三权限缺失通知的 AX 驱动（本机三项权限均已授权，TCC 切换属 §4.4）、目标锁定成功后的控制窗与 `scroll_prepared` |

实测记录见 `docs/validation/31-scroll-entry.md`。滚动入口的 `arcshot.overlay.scroll`
AX 标识已落地；权限通知与滚动控制窗的 AX 标识由 APP-EFFECT 与 UI 代码守护，
本机三项权限均已授权，因此 MAC-PERMISSION 的缺失通知分支保持 **Not tested**。

### #32 的稳定自动化入口

单帧完成闭环只走已确认接缝：core 的会话语义、app 的注入式调度与副作用、macos
的安全原生适配与 Quartz 决策门，以及真实 AX/文本诊断的 E2E。多帧拼接、唯一位移
验证、反向去重、实时预览与三秒完成预览由 #33 拥有，不在本票断言范围。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/scroll_loop.rs` | CORE-SESSION-46～60：输入安装→首帧请求、输入失败分级、可信首帧开启采集、帧失败分类（已知/不确定终态/内屏失效）、几何不符拒绝、无滚动单帧完成、音效偏好、复制失败保留可重试、Esc 返回原选区、输出期互斥、热键重启、输入/目标失效、迟到动作与重复帧；CORE-COORD-10：唯一的 display+crop→帧几何换算 | 真实 SCK/窗口/指针；#33 的拼接语义 |
| `crates/app/tests/scroll_frame_tasks.rs` | APP-ASYNC-13～16：单在途、有界绝对期限终态、worker panic 终态、取消后迟到结果仍交给版本栅栏；APP-FRAME-03～05：命名 sRGB 帧接纳、畸形宽高/stride/字节数、非不透明 alpha 拒绝 | 真实 SCK 请求状态确认 |
| `crates/app/tests/scroll_input_policy.rs` | APP-EFFECT-42～44：Finish 只在采集且输出空闲时路由、Cancel 在任一活动相位（含输出期）路由、动作只带当前 ticket | 真实物理按键投递（由 MAC-E2E-26 覆盖） |
| `crates/macos/src/scroll_input.rs`（单元测试） | MAC-SCROLL-INPUT（门缝）：选区内原始滚动放行/区外与首帧前拦住、点击与双击首击吞掉、Enter/Esc 按下释放归属、控制窗输入放行、拖动后 release-only drain、系统禁用 tap 视为保护丢失、失败完成后重开采集、只排第一个意图、未跟踪按钮视为保护丢失 | 真实 Quartz tap 安装/权限失效 |
| `crates/app/src/bin/scroll-fixture.rs`（自有 GPUI 精确像素夹具） | I-13 目标类 1：确定性像素真值、滚动/点击/按键文本回执；由 E2E 以独立进程启动，产品本身不启动它 | TextEdit/Safari 目标（§4.4 用户项） |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-26：准备→输入安装→可信首帧→采集的相位与像素真值探针；AX 控制状态与操作提示；单击拦截、原始滚动放行、Enter 完成复制并清理；Esc 放弃并返回原选区 | 三权限缺失通知（§4.4）、第三方目标、IME、视觉观感 |

单帧完成复用 `Document::commit_edits()` 与共享 `ComposeOutput`/PNGf 路径；输出前
版本栅栏、≤24,000,000 物理像素、≤16,384 单边、≤128 MiB PNG、最多 1 个 SCK 在途
与 1 个合成/编码任务由 core/image/app 既有接缝守护。原始帧队列上限 2 当前票只有
1 帧，不造队列；背压与拼接语义留给 #33。

实测记录见 `docs/validation/32-scroll-single-frame.md`。

### #33 的稳定自动化入口

双向拼接只走已确认接缝：image 的严格唯一位移/全重叠一致性验证与反向去重，core
的会话语义（原始帧待验证、已验证拼接文档替换完成文档），app 的有界单在途请求、
2 帧原始队列背压与固定预览原位刷新，以及自有 GPUI 精确像素夹具的真实 E2E。
不猜补、自由形变、模糊接缝或隐去缺口；不可验证即显式停止且不自动输出。三秒完成
预览、长图标注与部分结果恢复选择（#34）不在本票断言范围。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/image/tests/stitch.rs`（模块内单元测试） | IMAGE-STITCH-61～71：向下/向上唯一追加、反向经过已采区去重不再扩展、无位移重复视图去重、均匀内容歧义拒绝、重叠变化拒绝、无重叠缺口拒绝、几何不符拒绝、越过边长上限在分配前拒绝、预览确定性缩小且≤物理像素上限 | 真实 SCK 帧序列、真实长会话性能 |
| `crates/core/tests/scroll_loop.rs` | CORE-SESSION-61～66：采集帧先交验证、验证扩展替换完成文档、反向通过不改变完成文档、拼接拒绝停止且不输出、采集期帧失败分级同首帧（已知停止/不确定终态/内屏失效取消）、取消后迟到帧被忽略 | 真实原生故障注入 |
| `crates/app/tests/scroll_frame_tasks.rs` | APP-ASYNC-17～20：2 帧队列满时背压且不静默丢帧、提交策略要求闸门空闲且队列短、取消清空队列、闸门+队列最多 3 帧在途 | 真实 SCK 并发时序 |
| `crates/app/src/bin/scroll-fixture.rs` | I-13 目标类 1：确定性像素真值、慢速双向滚动与文本回执；本票以独立进程启动并由 E2E 驱动 | TextEdit/Safari 目标（§4.4） |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-27：右/左/选区内右下三个固定预览位置；右case慢速下滚扩展→反向回滚去重→Enter 完成复制；左 case 同链路；内侧 case 指针在预览上仍可原始滚动且其下捕获逐格真值正确、预览进入排除集、输出不含浮层 | 三秒完成预览、TextEdit/Safari、真实 TCC 切换、长会话资源峰值 |

拼接预览窗口在准备阶段一次性打开并加入固定排除集，采集中不新建/重建窗口；它
原生忽略鼠标事件，因此内侧预览上的原始滚动仍到达目标，同时 AX 仍可寻址。
自有窗口集合在采集前由「控制窗 + 固定预览窗」组成，`scroll_exclusion_fixed`
会同时打印两者；禁止采集中出现新窗口。

实测记录见 `docs/validation/33-scroll-stitch.md`。

### #34 的稳定自动化入口

显式部分结果只走已确认接缝：core 的 `AwaitingPartialChoice` 状态与接受/放弃动作、
app 的纯选择分类与控制窗 AX 按钮、自有 GPUI 精确像素夹具的原子跳变 E2E。已知
状态停止（内容变化、歧义重叠、目标/权限/输入保护/资源）且有已验证历史时，只
提供「使用已验证部分 / 放弃并返回原选区」两项显式选择，不自动输出；无已验证
结果时只放弃；原生不确定终态与内屏失效从不提供部分、禁止会话内重启。接受
部分后编码/复制失败保留结果供重试/放弃，不显示成功预览；成功复用共享输出路径。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/scroll_loop.rs` | CORE-SESSION-67～76：拼接拒绝带已验证历史时提供部分选择且不自动输出、全部已知采集期停止（内容/重叠/目标/输入/帧/资源）提供选择、采集前失败只放弃、放弃返回原选区且迟到结果忽略、接受后输出失败保留重试/放弃、显式取消不提供部分、不确定/内屏失效不提供部分、热键重启/重入不能篡改选择、成功后迟到结果不重复输出；CORE-SESSION-57/58/64/65 的采集期停止期望同步改为部分选择 | 真实原生故障注入、三秒完成预览 |
| `crates/app/tests/scroll_partial_flow.rs` | APP-EFFECT-45～49：已知失败族×有已验证结果恰好两项、无结果/不确定只放弃、标签与「不完整、确认后才复制」警告、选择映射到显式会话动作、接受后编码/复制失败仍提供重试/放弃；APP-ASYNC-21～22：等待选择时 tap 命令不路由、等待时不确定终态丢弃部分 | 真实 SCK 请求状态确认 |
| `crates/app/src/scroll_partial_flow.rs` | `partial_offered` / `partial_choices` / `partial_action` 纯分类与共享标签常量，被控制窗按钮与 E2E 断言复用 | GPUI 排版像素 |
| `crates/app/src/bin/scroll-fixture.rs` | I-13 目标类 1：按键 `j` 原子跳变 +1000 行（`fixture_jump` 回执），供无重叠缺口的受控负例；大滚轮会被拆成逐步事件并可能扩展，不用作缺口证据 | TextEdit/Safari 目标（§4.4） |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-28：慢速扩展后原子跳变造成 Gap 停止，AX 断言两项选择与不完整警告、点击前无输出；放弃返回原选区且无输出，接受经共享 Clipboard 路径输出；MAC-E2E-24 同步：采集期已知停止改走部分放弃回到覆盖层 | 真实 TCC 切换、第三方目标、动态/固定表头等其余负例的实机通过（仅注入式安全处理） |

控制窗在部分选择时以 `arcshot.scroll.partial.use` / `arcshot.scroll.partial.abandon`
两个 `button` 暴露选择，`arcshot.scroll.hint` 承载不完整警告；输入保护在进入
等待时退役（`scroll_input_retired`），控制窗与已验证预览保留，原始帧队列清空
（未验证帧丢弃）。

实测记录见 `docs/validation/34-scroll-partial-recovery.md`。

### #29 的稳定自动化入口

选项密度只走已确认接缝：core 的权威会话状态、app 的注入式偏好流、原生偏好域往返
与 AX 可观察的工具坞属性。切换密度不得改动工具、样式、选区、对象或进行中的编辑，
两种密度必须暴露同一套选项。精确像素与收起延迟是本票选定的布局数值，不是规格承诺。

| 文件 | 稳定 ID 与覆盖 | 不覆盖 |
| --- | --- | --- |
| `crates/core/tests/density.rs` | CORE-DENSITY-01～02：默认 Compact 与编解码、新会话恢复真实存储值；切换密度保持工具、每工具样式、选区、对象、文档版本与编辑状态不变 | GPUI 实际排版 |
| `crates/app/tests/preferences.rs` | APP-EFFECT-33～37：默认 Compact 与重启恢复；切换意图来自新的实际读取并持久化；不可读状态只报告；写入失败重读且不当作已生效；一次切换不产生多余偏好副作用 | 真实 `NSUserDefaults` 跨进程时序 |
| `crates/macos/src/preferences.rs`（单元测试） | MAC-START-29-02：真实偏好 suite 域的密度默认值、写入/新实例读取/删除回默认 | 用户自身偏好域 |
| `scripts/check-startup.sh`（`--smoke-test`） | MAC-START-29-01：七个菜单 action、`toggleDensity:` selector 与真实状态读取；`density_smoke=pass key=OptionDensity roundtrip=true restored=true default=Compact` | 人工菜单点击、像素密度 |
| `scripts/e2e-agent-desktop.py` | MAC-E2E-01（菜单七项）与 MAC-E2E-22：经状态菜单切换 Compact↔Standard，工具坞 `arcshot.overlay.density` 的 AX 值随之变化；同一套选项仍可寻址，工具/样式保持，跨一次真实重启保留，结束时还原用户原值 | 图标像素观感、悬停收起延迟、键盘单独访问选项 |

实测记录见 `docs/validation/29-density.md`。

权限提示替换竞态的原生冒烟记录见上述验证报告：真实 GPUI Esc 先入队、替换提示后
消费，旧 Esc 不关闭新提示，当前 Esc 正常关闭；该场景未纳入常驻自动化入口。

新增子用例必须记录稳定 ID、前置、步骤、预期与实际，不通过删除规格边界来减少
测试。模拟成功不能证明权限、焦点、IME、原生捕获或真实资源生命周期成功。

## 4. 端到端（agent-desktop）

端到端由代理通过 `agent-desktop` 执行，不再要求用户按脚本点击。工具实测版本
`agent-desktop 1.1.0`（envelope `2.6`）；先读 `agent-desktop skills get desktop`
与 `agent-desktop skills get platform`，再用 `agent-desktop status` 确认权限与
平台，然后才驱动。`snapshot`/`find` 的引用只在一次快照内有效，动作结果必须按
`error.code` 与 `disposition.retry` 分支，不得据失败文案猜测。

### 4.1 前置、实例与串行

- 启动 `agent-desktop` 的终端必须持有辅助功能（macOS 27 起名 Device Control and
  Data Access）权限；屏幕捕获路径另需屏幕录制权限。权限缺失先修权限，不得把
  未授权导致的拒绝记成产品失败。
- 每轮 E2E 只允许一个 ArcShot 实例。`agent-desktop list-apps --app "ArcShot"`
  必须只返回被测 bundle：Debug 为 `dev.arcshot.app.debug` / `ArcShot Debug`，
  Release 为 `dev.arcshot.app` / `ArcShot`。多实例会让状态项与菜单引用含义不明。
  MAC-E2E-16 的跨重启用例会在同一轮内按顺序重启两次，每次仍只有一个实例；
  各次启动的 stderr 分别写入 `target/e2e-agent-desktop-<configuration>[-restart-<n>].log`。
- E2E 串行：同一时间只有一个代理驱动桌面。菜单命令、热键投递与 `--headed` 按键
  都会改变前台状态，并行会互相污染证据。
- 全局窗口清单（不带 `--app` 的 `list-windows`）在本机不稳定（反复
  `global_window_owner_churn`，即使 ArcShot 未运行）：一律用 `list-windows --app`，
  并且不使用需要全局清单的 `snapshot --window-id`。

### 4.2 可驱动层与观察合同

| 层 | `agent-desktop` 实测能力 | 用途 |
| --- | --- | --- |
| AppKit 状态项与 NSMenu | 状态项与六个菜单项以 role `menuitem` 暴露，`native_id` 就是 selector（`startCapture:`、`hotkeySettings:`、`screenRecordingPermissions:`、`toggleCompletionSound:`、`toggleLaunchAtLogin:`、`terminate:`）；可 headless `click`，不需要打开菜单、不需要前台 | 驱动全部菜单命令与两个偏好开关 |
| 带标题栏的 GPUI 窗口（快捷键设置、权限提示） | 出现在 `list-windows --app`（尺寸/`is_focused`）；窗口内按 #53/#54 有可寻址节点：`group` 根节点带可见名称，文本是 `statictext`（值即文本），控件是 `button` 且 `native_id` 为稳定 `AXIdentifier`（`arcshot.notice.close`、`arcshot.hotkey.recorder`…） | 读窗口内容并按名字/标识驱动 |
| 无标题栏的冻结覆盖层 | 仍不出现在 `list-windows`（`titlebar: None`，非激活 borderless），但按 #55 有可寻址 AX 子树：`group "ArcShot 截图覆盖层"` + `button arcshot.overlay.cancel` | 用 `snapshot --app` 观察与取消，不靠 `--headed` Esc |

基座是 `gpui-pre` 0.3.11（ADR-0001），它上报 accessibility 子树；产品在 #53/#54/#55
给三个窗口加了 `id` + `role` + `aria_label` + `accessibility_id` + `on_a11y_action`。
E2E 的观察合同因此是**双通道**：

1. **驱动**用 AX：状态项与六个菜单项，以及窗口内的 `button` 节点（按 `native_id`
   或名字命中）。
2. **状态**以产品 stderr 诊断行为准（`capture_phase from=… to=… action=…`、
   `capture_request_us=`、`capture_probe_grid`、`capture_wake_window_submit_us=`、
   `capture_edit action=… version=… annotations=… selection=…`、
   `menu_hotkey_settings`、`hotkey_register …`、`app_active surface=frozen`、
   `preferences_launch completion_sound=…`、`completion_sound enabled=… persisted=true`、
   `login_item_status state=… source=…`、`login_item_set enabled=… outcome=…`）。
   `capture_phase` 是会话相位的唯一
   真相：开始、暂停、取消、重启与失败都各有一行，不打印帧内容。冻结覆盖层必须先在
   日志中出现 `capture_wake_window_submit_us=`，取消必须出现 `to=Resident` 的相位
   行，才能声称会话真的离开了冻结态。`capture_edit` 只在重选、拖动提交、撤销与重做
   后打印版本、标注数量与选区逻辑几何，不打印像素。
3. **窗口**断言只用 `list-windows --app`。驻留时它返回空列表，表示没有带标题的
   自有窗口；冻结覆盖层存在时它同样为空，因为该窗口无标题栏不被枚举。所以这个断言
   既不能证明覆盖层存在，也不能证明它已关闭，只能证明没有可捕获的自有窗口。
4. **按键**分两类。全局组合用不带 `--headed` 的 `press`（physical 投递，不要求
   ArcShot 在前台，但必须先用一个未注册组合做阴性对照）；窗口内按键（录制新组合、
   Esc）只能用 `press … --headed` 投到前台应用，并且必须先确认前台真的是被测应用：
   冻结覆盖层用产品自己的 `app_active surface=frozen`（覆盖层无标题、`snapshot
   --surface focused` 对它返回 `WINDOW_NOT_FOUND`）；带标题窗口用
   `snapshot --surface focused` 的 `app` 等于被测显示名。headless `press --app` 在
   无焦点 AX 元素时必然 `ACTION_FAILED`；`--headed` 坐标点击不属于稳定断言，不能用
   于验收。这个前置只吸收焦点竞态，不放宽对产品行为的断言。同一按键在焦点竞态下
   最多重投 4 次，每次都重新确认前置（冻结层重新等 `app_active surface=frozen`，
   带标题窗口重新读 `snapshot --surface focused`）；只有产品自己的诊断行出现才算送达，
   未按到仍判失败，断言本身不放宽。
5. **AX 语义**：`AXPress` **不会**触发 `on_click`，两条路径都必须显式注册
   `on_a11y_action(AccessibleAction::Click, …)`，且两条路径入同一个 `UiQueue`；每次
   AX 激活另写一行 `a11y_action=<surface>.<control> action=Click`，E2E 以该行证明
   动作真的到达产品。
6. **懒激活**：Accessibility 在第一个辅助技术客户端连接时才建立，窗口刚打开的第一
   次 `snapshot --app` 可能是空树；必须先重读再下结论（脚本里的 `wait_node`）。

### 4.3 稳定用例

本轮实测环境：Debug 与 Release 构建、正式 bundle 可执行文件、内建屏
`1470x956 scale=2`、屏幕录制权限已授予。基座迁移见
`docs/validation/e2e-agent-desktop.md`，可访问性接线见
`docs/validation/56-a11y-e2e.md`。

| 稳定 ID | 前置与步骤 | 必须观察 | 明确不覆盖 |
| --- | --- | --- | --- |
| MAC-E2E-01 | 启动被测可执行文件（stderr 重定向到日志）；`agent-desktop snapshot --surface menu-bar-extras --app "ArcShot Debug"` | 恰好一个状态项（role `menuitem`、name `ArcShot`）；其菜单七项依次为 开始截图 / 截图快捷键… / 屏幕录制权限… / 选项密度 / 完成音效 / 登录时启动 / 退出 ArcShot，`native_id` 与 selector 一致；不要求 ArcShot 在前台 | 图标像素观感、Dock 策略、勾选状态的像素表现 |
| MAC-E2E-02 | 驻留时 `agent-desktop list-windows --app "ArcShot Debug"` | 返回空列表（无可捕获自有窗口） | 未标题覆盖层；见 §4.2 |
| MAC-E2E-03 | `find --surface menu-bar-extras --role menuitem --name "开始截图"` 取引用后 headless `click` | 新增 `capture_request_us=`、`capture_probe_grid` 与 `capture_wake_window_submit_us=` | 覆盖层内容、选区、输出 |
| MAC-E2E-04 | 驻留时先 `agent-desktop press <未注册组合>`，再 `press <已注册组合>`（组合从日志 `hotkey_launch config=` 翻译） | 未注册组合不产生 capture 序列；已注册组合产生与 MAC-E2E-03 相同的序列；不要求前台应用是 ArcShot | 系统冲突、冲突恢复、真实键盘布局与按键重复率 |
| MAC-E2E-05a | MAC-E2E-03 后等产品报告 `app_active surface=frozen`，再 `agent-desktop press escape --headed`，再取 `list-windows --app` | `press` 返回 `ok` 且 `delivery=delivered_unverified`；`list-windows --app` 为空；随后开始截图仍产生新的 capture 序列 | 冻结层 NSWindow 是否被销毁（无外部探针） |
| MAC-E2E-05b | 同上，读 Esc 之后的 `capture_phase` 行 | 出现 `from=Frozen to=Resident action=CancelAttempt` | NSWindow 销毁级证据；GPUI 键处理细节（CORE-SESSION 已覆盖） |
| MAC-E2E-06 | 点击 截图快捷键… | 日志 `menu_hotkey_settings`；`list-windows --app` 出现一个可寻址窗口（`480x362`、标题 `ArcShot Debug`、`is_focused: true`） | 窗口内录入、保存、Esc 取消（无 AX 节点，策略由 APP-EFFECT 覆盖） |
| MAC-E2E-07 | 点击 屏幕录制权限… 后 `press escape --headed` | 出现 `560x282` 自有窗口；Esc 后 `list-windows --app` 为空 | 真实 TCC 提示、设置返回重检、运行中撤权 |
| MAC-E2E-08 | 点击 退出 ArcShot | 进程退出；`agent-desktop find --surface menu-bar-extras --app "ArcShot Debug"` 返回 `APP_NOT_FOUND`；`list-apps --app "ArcShot"` 为空 | 系统按 ID 重启、登录项、状态项像素残留 |
| MAC-E2E-09 | 启动前后 `agent-desktop list-apps --app "ArcShot"` | 只出现被测 `bundle_id` 与显示名（Debug/Release 各一） | 系统按 bundle ID 解析到哪一份拷贝 |
| MAC-E2E-10 | 点击 屏幕录制权限…，等 AX 建立后 `snapshot --app` | `group "ArcShot 权限提示"`、至少两个 `statictext`（标题与正文可读）、一个 `button` 带 `arcshot.notice.*` 标识；AX 激活后日志出现 `a11y_action=arcshot.notice.<id> action=Click`，窗口消失 | 未授权的变体（需改 TCC）；打开系统设置的分支不驱动 |
| MAC-E2E-11 | 点击 截图快捷键…，`snapshot --app` 后按节点驱动；每次投递录制组合前先确认 `snapshot --surface focused` 的 `app` 是被测显示名 | 读到当前组合与录入区名称；AX 触发录入 → 名称随状态变化（候选/不合法）→ AX 保存写入 `hotkey_store value=<新组合>`，再用同一路径恢复原组合并以 AX 关闭窗口 | 冲突恢复、暂挂失败（APP-EFFECT 覆盖） |
| MAC-E2E-12 | 开始截图后 `snapshot --app` | `group "ArcShot 截图覆盖层"` + `button arcshot.overlay.cancel`；AX 激活后 `a11y_action=arcshot.overlay.cancel action=Click` 与 `capture_phase … to=Resident`，无残留自有窗口 | 覆盖层的 `list-windows` 枚举（见 §4.5）；选区移动/调整/重选与撤销（控制点无 AX 节点，不能用 `--headed` 坐标）；选区/标注与保存（保存按钮只在有效选区时成为 `button arcshot.overlay.save`，选区尚无 AX 节点） |
| MAC-E2E-13 | 冻结覆盖层 `snapshot --app` 后按 `arcshot.overlay.tool.*` 驱动 | `group "标注工具"` 含 Select（checked=true）与 Rectangle（checked=false）；AX 激活 Rectangle 后日志 `capture_edit action=SelectTool … tool=Rectangle` 且 checked 状态互换；无选中对象时不出现删除按钮 | 真实指针选择工具、工具图标观感 |
| MAC-E2E-14 | MAC-E2E-13 后 `hover @<rectangle ref> --headed` 再 `snapshot` | 选项条按 `arcshot.overlay.option.rectangle.*` 可寻址；AX 激活 5 点线宽与蓝色后 `capture_edit action=RectangleOption … style=stroke=#007AFF width=5`，版本不变，选项 checked 状态更新 | 填充开关的像素、选中对象改样式（无对象可选中）、真实指针悬停手感 |
| MAC-E2E-15 | 读启动日志 `preferences_launch completion_sound=<初始值>`；分别 `find … --name "完成音效"` 后 headless `click` 两次 | 每次出现 `completion_sound enabled=<相反值或初始值> persisted=true`；初始值先被翻转再恢复 | 真实音量/静音观感、通知（不请求通知权限）；偏好域取值另由人工 `defaults read` 交叉核对 |
| MAC-E2E-16 | 把完成音效切到与初始值相反的取值；点击 退出 ArcShot 后重启同一可执行文件；再切回初始值并重启 | 两次重启的 `preferences_launch completion_sound=` 分别等于重启前的相反值与初始值；结束时用户偏好已还原；每次只有一份实例 | 崩溃/强制退出后的写入时序、注销重启（登录项改动不在 E2E 内） |
| MAC-E2E-17 | MAC-E2E-01 的 `snapshot --surface menu-bar-extras` 打开状态菜单后读日志 | 出现 `login_item_status state=… source=menu`，状态属于 Enabled/Disabled/RequiresApproval，且与启动读取 `source=launch` 一致；整段日志没有 `login_item_set `，即读取菜单从不注册/注销登录项 | 真实切换（注册/注销会改动系统登录项）、失败提示窗口、系统设置批准流程 |
| MAC-E2E-18 | `snapshot --app` 后 AX 依次激活 `arcshot.overlay.tool.line` 与 `.arrow` | 四个工具均可寻址；每次激活出现对应 `a11y_action=`、`capture_edit action=SelectTool … tool=Line/Arrow`，checked 仅在激活项为 true | 真实指针选择工具、图标观感 |
| MAC-E2E-19 | 悬停 Line 工具后 AX 激活虚线、2 点线宽与绿色 | 选项条按 `arcshot.overlay.option.line.*` 可寻址；日志依次出现 `capture_edit action=LineOption … dash=true`、`width=2`、`stroke=#34C759`，checked 状态更新且版本不变 | 虚线像素、选中线改样式（无对象可选中） |
| MAC-E2E-20 | 悬停 Arrow 工具后 AX 激活内凹、5 点线宽与橙色 | 选项条按 `arcshot.overlay.option.arrow.*` 可寻址；日志依次出现 `capture_edit action=ArrowOption … inward=true`、`width=5`、`stroke=#FF9500`，checked 状态更新且版本不变 | 箭头像素、选中箭头改样式 |
| MAC-E2E-21 | MAC-E2E-19/20 后悬停 Rectangle 工具 | Rectangle 的 5 点线宽与蓝色仍为 checked，证明每工具样式记忆互不影响 | 真实悬停手感、视觉排版 |
| MAC-E2E-22 | 读启动日志 `preferences_launch density=`；冻结后保持 Rectangle 与一个已改样式；状态菜单点击 选项密度 两次并重启一次 | 每次出现 `option_density density=… persisted=true`；工具坞 `arcshot.overlay.density` 的 AX `value` 变为对应密度；`arcshot.overlay.option.rectangle.*` 仍可寻址、工具与样式保持；重启后 `preferences_launch density=` 等于切换值；结束时还原用户原值 | 密度图标/间距的像素观感、悬停收起延迟、键盘单独访问选项 |
| MAC-E2E-23 | MAC-E2E-21 后 AX 激活 `arcshot.overlay.tool.blur`，`hover @<blur ref> --headed` 再 `snapshot` | 五个工具均可寻址；激活后 `capture_edit action=SelectTool … tool=Blur` 且 checked 互换；选项条按 `arcshot.overlay.option.blur.*` 可寻址，默认仅 `medium` 为 checked；AX 激活 `weak`/`strong` 后依次出现 `capture_edit action=BlurOption … level=weak`、`level=strong`，checked 状态更新 | 模糊像素、选区绘制、模糊对象创建/拖动、tooltip 文本（`agent-desktop 1.1.0` 不暴露 GPUI tooltip/`aria_description`） |
| MAC-E2E-24 | 开始截图冻结后 `snapshot --app`；先对 `arcshot.overlay.scroll` 发 AX 点击（无选区），再以 `drag --from-xy 400,300 --to-xy 700,500 --headed` 作为前置创建选区，最后 AX 激活 `arcshot.overlay.scroll` | 入口始终以 `button` 可寻址；无选区时点击返回 `POLICY_DENIED` 且不产生 `a11y_action=arcshot.overlay.scroll`；拖拽后出现 `capture_edit action=SelectRegion` 与 `selection=Some([…])`，入口点击出现 `a11y_action=arcshot.overlay.scroll action=Click` 与 `scroll_permission screen_recording=… input_monitoring=… accessibility=… missing=…`；随后出现 `scroll_target…` 或 `scroll_refused reason=Target …`；区域覆盖层与选区保留 | 三权限缺失通知的 AX 驱动（本机三项权限均已授权，TCC 切换属 §4.4）；目标锁定成功后的控制窗与 `scroll_prepared`；`--headed` 拖拽只是选区前置，不作为指针手势的验收证据 |
| MAC-E2E-25 | MAC-E2E-23 后 AX 激活 `arcshot.overlay.tool.text`，`--headed` 拖拽建选区并点击创建文字（前置），`hover @<text ref> --headed` 再 `snapshot` | 六个工具均可寻址；激活后 `capture_edit action=SelectTool … tool=Text` 且 checked 互换；文字输入 `arcshot.overlay.text.input` 可寻址；选项条按 `arcshot.overlay.option.text.*` 可寻址，默认字号 `16`；AX 激活 `size.up` 后 `capture_edit action=TextOption … size=16.5` 且字号值变为 `16.5`；AX 激活 `color.blue` 后 `stroke=#007AFF` 且 checked 更新；点击别处出现 `capture_edit action=TextCommit … annotations=1` | 真实指针文字创建/拖动、直接输入字号、原生系统字体/IME 候选框与焦点、预览像素 |
| MAC-E2E-26 | 由脚本以独立进程启动自有 GPUI 精确像素夹具 `arcshot-scroll-fixture`；开始截图后 `--headed` 拖拽建选区（前置），AX 激活 `arcshot.overlay.scroll`；采集后 `mouse-click`/`mouse-wheel`/`press return --headed`/`press escape --headed` 作为物理输入前置 | `scroll_target window_id=…`、`scroll_prepared`、`scroll_exclusion_fixed`、`scroll_input installed=true`、`scroll_frame_request_us=`、`scroll_phase … to=Collecting`、`scroll_collecting`；`arcshot.scroll.status` 为“可以滚动”且 `arcshot.scroll.hint` 可寻址；`capture_probe_grid` 与夹具像素真值逐格相等；夹具日志无 `fixture_click_received`、出现 `fixture_key_received` 与 `fixture_scroll`；`output_publish target=Clipboard` 后 `scroll_input_retired … drain_forced=false`；第二轮 Esc 出现 `from=Collecting to=Inactive action=CancelScroll` 且 `arcshot.overlay.save` 重新可寻址 | 真实 TCC 切换、TextEdit/Safari 第三方目标、IME 候选框、视觉观感 |
| MAC-E2E-27 | 由脚本启动同一夹具；分别以右侧、左侧、选区内右下三个选区进入滚动；右 case 慢速下滚 4 次、反向回滚 4 次后 Enter 完成；左 case 同链路；内侧 case 指针在固定预览上滚 3 次后 Enter 完成 | `scroll_preview placed=right/left/inner frame=[…]`；`scroll_exclusion_fixed … preview=<id>`；`scroll_stitch extended offset=… added_rows=…`；反向 `scroll_stitch duplicate offset=…` 且扩展计数不增；夹具出现 `fixture_scroll`（含指针在预览上的情况）；每个位置的 `capture_probe_grid` 与该偏移的夹具像素真值逐格相等（其下捕获正确、预览不入图）；三次均 `output_publish target=Clipboard` 且 `from=Frozen to=Resident action=OutputCompleted` | 三秒完成预览、TextEdit/Safari、真实 TCC 切换、长会话资源峰值、视觉观感 |
| MAC-E2E-28 | 由脚本启动同一夹具；选区内慢速下滚 2 次扩展已验证历史，按键 `j` 触发夹具原子跳变 +1000 行造成无重叠 Gap 停止；AX 读控制窗选择；分别点击放弃与接受 | `scroll_stitch_refused … gap` 与 `scroll_phase … to=AwaitingPartialChoice`；`scroll_partial_choice available=true … pixels=460x424`；点击前无 `output_publish`；`arcshot.scroll.partial.use` / `.abandon` 均可寻址且 `arcshot.scroll.hint` 含“不完整”；放弃出现 `scroll_partial_choice action=abandon` 与 `action=AbandonScrollPartial`，覆盖层与选区返回且无输出；接受出现 `action=accept` 后才 `output_publish target=Clipboard` 与 `from=Frozen to=Resident action=OutputCompleted` | 其余负例（动态/固定表头/重复低纹理/横向·对角·嵌套/目标·权限·输入保护失效）的实机通过仅由注入式用例证明安全处理；真实 TCC 切换、TextEdit/Safari、原生故障注入与视觉观感 |

入口是 `scripts/e2e-agent-desktop.py [--release]`：它先门禁 `agent-desktop status`
与在跑实例，再用 `scripts/build-app.sh` 构建、启动被测可执行文件并把 stderr 写入
`target/e2e-agent-desktop-<configuration>.log`（MAC-E2E-16 的两次重启与 MAC-E2E-22 的三次
重启另写 `-restart-1…-5.log`，不清空前一份），然后逐条断言上表并打印
`mac_e2e_XX=pass …`。失败即非零退出，不得改成“人工看一眼”。手工排查时可用同类
单条命令（引用不得跨快照复用）：

```sh
agent-desktop status
agent-desktop snapshot --surface menu-bar-extras --app "ArcShot Debug"
agent-desktop find --surface menu-bar-extras --app "ArcShot Debug" \
  --role menuitem --name "开始截图"
agent-desktop click @<snapshot_id>:e<N>
agent-desktop list-windows --app "ArcShot Debug"
agent-desktop press escape --headed
```

### 4.4 仍属用户或人工的项目

- macOS 的 TCC 授权与「系统设置」面板：由用户在系统提示上确认；代理只读
  `agent-desktop status` 与产品自己的提示流程，不代替用户授权，也不擅改权限。
- 视觉与审美判断：状态项图标观感、标注排版审美、贴边布局与密度是否可接受。
- 真实第三方输入法（IME 候选窗与组合过程）、多显示器几何与物理输入设备差异。
- 除上述三类之外，菜单命令、热键投递、窗口出现/消失、退出清理与日志诊断都由代理
  执行，不得再要求用户按脚本点击。

### 4.5 观察缺口与需要的接缝

下列缺口不转交给用户，而是产品侧要补的观察接缝；补上之前相关子用例保持
**Not tested**：

- 已落地：会话每次相位转换（`Resident` / `CheckingPermission` / `Preparing` /
  `Draining` / `Frozen` / `RelaunchRequired`，附语义动作名）各有一行纯文本 stderr
  事件，因此 MAC-E2E-05b 与重启/失败路径不再依赖坐标或截图。
- 已落地（#30）：冻结覆盖层在打开后第一次真正成为前台应用时写一行
  `app_active surface=frozen`，E2E 用它挡在 `--headed` Esc 之前；带标题窗口改用
  `snapshot --surface focused` 的 `app` 判断前台。两者只消除焦点竞态，不替代
  `capture_phase` / `hotkey_store` 等行为断言。
- 已落地（#30）：两个偏好项是普通菜单项（`toggleCompletionSound:` /
  `toggleLaunchAtLogin:`），菜单打开时 `validateMenuItem:` 读真实偏好与
  `SMAppService` 状态并写一行 `login_item_status … source=menu`；完成音效切换写
  `completion_sound enabled=… persisted=true`。`AXPress` 只驱动 action，实际
  `SMAppService` 变更仍由 coordinator 执行，因此 E2E 不点击“登录时启动”。
- 已落地（#29）：密度是 `arcshot.overlay.density` 的 AX `statictext` 值
  （`Compact` / `Standard`），状态菜单切换另写一行
  `option_density density=… persisted=true`，启动时写 `preferences_launch density=…`。
  `agent-desktop` 不暴露 NSMenuItem 勾选状态，因此工具坞属性是唯一可驱动的密度断言。
- 已落地（#31）：滚动入口 `arcshot.overlay.scroll` 始终以 `button` 可寻址；无选区时
  不注册 `on_a11y_action`，点击返回 `POLICY_DENIED`；有选区时 AX 点击写一行
  `a11y_action=arcshot.overlay.scroll action=Click`。三权限预检写一行
  `scroll_permission screen_recording=… input_monitoring=… accessibility=… missing=…`；
  目标锁定写 `scroll_target window_id=… owner=… frame=[…]`，失败写
  `scroll_target_failure reason=…` 与 `scroll_refused reason=…`；相位写
  `scroll_phase from=… to=… action=…`。权限通知与滚动控制窗的 AX 标识
  （`arcshot.scroll.permission.*`、`arcshot.scroll.recheck`、`arcshot.scroll.cancel`、
  `arcshot.scroll.close`、`arcshot.scroll.status`、`arcshot.scroll.control.cancel`）已落地，
  但本机三项权限均已授权，缺失通知分支不能由 E2E 驱动，保持 **Not tested**。
  滚动入口的选区前置由 `--headed` 拖拽创建（`capture_edit action=SelectRegion`
  证明），拖拽只是前置，不作为指针手势的验收证据。
- 已落地（#32）：采集控制窗 `arcshot.scroll.status` 在 `ScrollPhase::Collecting` 时
  为“可以滚动”，并以 `arcshot.scroll.hint` 暴露简洁操作提示（无百分比/ETA/暂停）；
  `arcshot.scroll.control.cancel` 始终可 AX 激活。相位与副作用另有文本诊断：
  `scroll_control_window number=…`、`scroll_exclusion_fixed prepared=… collecting=…`、
  `scroll_input installed=true owned_windows=…`、`scroll_frame_request_us=…`、
  `scroll_collecting ticket=…`、`output_publish target=Clipboard bytes=…`、
  `scroll_input_retired ticket=… scrolls=… clicks_blocked=… keys_owned=… drain_forced=…`。
  失败分类写 `scroll_input_failure reason=…`、`scroll_target_intact=false reason=…`、
  `scroll_refused reason=Input/Frame …`。自有 GPUI 精确像素夹具由脚本独立启动，
  只写文本回执；产品本身不启动它、不保存用户截图。
- 已落地（#33）：固定实时预览窗在准备阶段一次性打开，原生忽略鼠标事件，AX 标识
  `arcshot.scroll.preview`（role image）；采集中只原位替换图像。自有窗口集合从
  #32 的单个控制窗变为「控制窗 + 固定预览窗」，因此 `scroll_exclusion_fixed` 现打印
  `prepared=3 collecting=2 control=… preview=…`（数值随可见窗口数而定），任一窗口
  缺席或新增都拒绝启动采集；预览 id 同时进入每条 `capture_scroll_region` 的排除数组。
  拼接诊断写 `scroll_stitch extended offset=… added_rows=… top=… bottom=…
  extensions=… duplicates=…`、`scroll_stitch duplicate offset=…`、
  `scroll_stitch_refused reason=…`；预览刷新写 `scroll_preview_updated place=…
  pixels=… document=… extensions=… duplicates=…`；预览放置写
  `scroll_preview placed=right|left|inner frame=[…]`。原始帧队列与背压由
  APP-ASYNC-17～20 守护。
- 已落地（#34）：已知状态停止且有已验证历史时，控制窗切换为显式部分选择：
  `arcshot.scroll.partial.use`（使用已验证部分）与
  `arcshot.scroll.partial.abandon`（放弃并返回原选区）两个 `button`，
  `arcshot.scroll.hint` 承载“已验证部分不完整；确认后才会复制”，`arcshot.scroll.status`
  承载停止原因；诊断写 `scroll_partial_choice available=true|false … pixels=…`、
  `scroll_partial_choice action=accept|abandon …` 与相位
  `to=AwaitingPartialChoice`。等待时输入保护退役（`scroll_input_retired`），
  控制窗与已验证预览保留、未验证队列清空；无已验证结果时写
  `available=false` 并直接返回覆盖层（只放弃）。夹具按键 `j` 的原子跳变写
  `fixture_jump offset_points=…`，供 MAC-E2E-28 的 Gap 受控停止。
- 仍缺：登录项切换（注册/注销）与失败提示窗口的可驱动证据。它会改动用户系统登录项
  并可能要求系统设置批准，E2E 与冒烟都不执行；成功/失败/不可读分支由 APP-EFFECT-28～32
  的注入结果覆盖，MAC-START 的人工项保持 **Not tested**。
- 仍未落地：覆盖层的窗口级枚举。`WindowKind::PopUp` + `titlebar: None` 的非激活
  borderless 窗口在 0.3.11 下仍不出现在 `list-windows`；#55 改以 `snapshot --app`
  证明“可寻址”，NSWindow 销毁级证据仍缺外部探针。
- 已落地（#53/#54/#55）：权限提示、快捷键设置与冻结覆盖层的元素都有
  `id` + `role` + `aria_label` + `accessibility_id` + `on_a11y_action`，
  `snapshot --app` 可见、可驱动。#23 的“保存”控件在有效选区且输出空闲时注册为
  `button arcshot.overlay.save`，但打开保存面板需要有效选区，而选区本身尚无 AX
  节点（需先完成拖拽）。因此 MAC-RECOVERY 的保存面板显示/取消/覆盖写仍为
  **Not tested**，不能用 `--headed` 拖拽补足。#25/#26/#27 另外标注了左侧工具坞
  （`arcshot.overlay.tool.*`）、矩形/直线/箭头/模糊各自的悬停选项
  （`arcshot.overlay.option.rectangle.*`、`.line.*`、`.arrow.*`、`.blur.*`）与有选中对象时的删除
  按钮（`arcshot.overlay.delete`），并用 `aria_toggled` 让 macOS AX 暴露选中态
  （`checked`）。#27 的模糊 tooltip 与 `aria_description` 是真实 GPUI 文本，但
  `agent-desktop 1.1.0` 的 snapshot/find 不暴露 tooltip 或 description，因此 E2E 只断言
  AX id 与 `capture_edit` 诊断，tooltip 文案由 `ui::blur_tooltip_tests` 守护。#28 另外标注了
  文字工具（`arcshot.overlay.tool.text`）、文字色板/字号步进
  （`arcshot.overlay.option.text.*`）、字号值（`arcshot.overlay.text.size`）与文字输入
  （`arcshot.overlay.text.input`）。**尚未**标注的是区域选区与八个调整控制点、标注对象本身与其控制点
  （含线/箭头的两个端点）；#33 已把滚动预览标注为 `arcshot.scroll.preview`
  （role image）。#24 已画出控制点并把移动/调整/重选接到 core，但控制点
  不是 AX 节点；#25/#27 的对象绘制、移动、调整、删除仍依赖指针拖拽或只有选中对象后才出现
  的删除按钮，而选中对象又只能通过指针命中。这些产品面落地时必须沿用 §4.2 的约定，
  否则 MAC-REGION / MAC-ANNOTATION 的指针手势仍无法进入 E2E。
- 本机全局窗口清单不稳定、`snapshot --window-id` 不可用：E2E 不得依赖它；需要窗口
  级引用时先用 `list-windows --app` 取 id。

在上述接缝落地前，禁止用 `--headed` 坐标点击、`screenshot` 像素比较或人工观察补足
这些用例并记为 Pass。

## 5. macOS 实机矩阵（代理执行）

实机使用正式产品构建，记录环境与执行主体。以下矩阵均为发布前必测；当前只有
MAC-START-01、PRIVACY-01 与 §4.3 的窄范围 E2E 通过，其余不得继承原型结论。
可驱动部分由代理通过 `agent-desktop` 执行；只有 §4.4 的项目留给用户，未自动化的
行记为 **Not tested**，不用用户代跑充数。

| 稳定用例组 | 实机范围 | 本轮 `agent-desktop` 可行性 |
| --- | --- | --- |
| MAC-START | 菜单驻留、无 Dock 主窗口、全部菜单命令、默认/自定义热键、冲突恢复、偏好跨重启、真实登录项状态/失败 | 部分：菜单、默认热键、退出、两个偏好项与完成音效跨重启见 §4.3（MAC-E2E-01/15/16/17），热键录入与保存由 MAC-E2E-11 驱动；热键冲突恢复与登录项真实切换/失败仍不可驱动（会改动系统状态） |
| MAC-PERMISSION | Region Screen Recording 引导；Scroll 三权限预检、设置返回重检、运行中失效；拒启/失效不输出 | 部分：提示窗口出现/关闭与 AX 驱动见 MAC-E2E-07/10；Scroll 三权限预检诊断与入口闸门见 MAC-E2E-24；本机三项权限均已授权，缺失通知/设置返回重检/运行中失效属 §4.4，保持 **Not tested** |
| MAC-REGION | 单内屏框选、移动/调整、重选/撤销、完成/保存/取消、Esc、快捷键重启、2x 几何 | 部分：冻结/取消与菜单/热键见 §4.3；选区拖动、八向调整、重选与 `Cmd+Z` 只有指针/键盘手势，控制点无 AX 节点，保持 Not tested |
| MAC-ANNOTATION | 全工具、命中/控制点、层叠/历史、越界、Blur、密度、悬停/焦点与贴边布局 | 部分：五个工具与矩形/直线/箭头样式选项、模糊三档可 AX 驱动；绘制、对象命中与端点拖动仍无 AX 节点 |
| MAC-TEXT | 中文 IME、拉丁文字、emoji、多行；提交/Finish/保存不丢组合文字；空文本与撤销 | 否：文本编辑尚未实现；IME 候选窗另属 §4.4 |
| MAC-SCROLL-POSITIVE | 自有 GPUI 夹具、TextEdit 固定文档、Safari `file://` 静态页分别完成启动、慢速双向、反向去重、完成/取消/输出 | 未实测：目标应用侧滚动可由 `agent-desktop scroll`/`press` 投递，产品侧结果仍只能由日志证明 |
| MAC-SCROLL-UI | 右/左/选区内预览、目标内容不被浮层污染、多自有窗口全部精确排除 | 否：浮层尚未实现，落地时须补 AX 标注 |
| MAC-SCROLL-INPUT | 单帧、Enter、双击首击/完成、单击拦截、预览上滚动、前台/hit-test、Esc/重启/清理 | 部分：自有 GPUI 夹具上 MAC-E2E-26 验证了准备→输入保护→可信首帧→采集→Enter 完成/Esc 放弃、单击拦截、原始滚动放行、tap 计数器与清理；双击完成与全局热键重启只有注入式门缝测试（APP-EFFECT-42～44、CORE-SESSION-56），真实物理双击/热键重启仍 **Not tested** |
| MAC-SCROLL-NEGATIVE | 动态、固定浮层、重复/低纹理、无重叠跳变、横向/对角/嵌套滚动；无法验证即停止 | 否：判定依赖视觉结果，需 `screenshot` 或人工（§4.4） |
| MAC-TARGET-FAIL | 目标移动/关闭/前台变化、几何/尺度变化、遮挡、输入保护/权限失效、内屏失效 | 部分：目标应用侧可用 `agent-desktop` 窗口命令制造，产品侧判定仍靠日志 |
| MAC-NATIVE-FAIL | 资源上限、SCK 错误/超时、worker 异常；普通失败与禁止输出/重启的不确定终态分离 | 否：现有 Rust/原生接缝覆盖，E2E 无法注入 |
| MAC-COLOR-OUTPUT | 命名 sRGB 全链路、PNGf 剪贴板、内存解码尺寸/像素/颜色、第三方粘贴 smoke、显式保存字节一致 | 部分：本机 `agent-desktop 1.1.0` 无剪贴板子命令，字节比对仍需脚本 |
| MAC-RECOVERY | 保存取消/失败、复制失败/重试/放弃、3 秒预览、长图整图标注、取消保留旧剪贴板 | 部分：保存取消/失败/重试/放弃与文件写入由 core/app/原生自动化覆盖，`save_file_smoke` 验证真实写入；面板显示/取消/覆盖写需有效选区，选区尚无 AX 节点（§4.5），保持 Not tested |
| MAC-LIFETIME-PRIVACY | 不重复输出/污染会话；回调/窗口/tap/图像引用清理；无自动截图落盘、无运行期联网 | 部分：退出清理见 MAC-E2E-08，联网见 PRIVACY-01；资源引用仍需进程级检查 |

权限切换若先触发焦点停止，只能记录实际时序，不能冒称“运行中直接撤权”已验证。
负面用例通过只表示安全处理，不外推为任意应用、动态内容、显示器或滚动速度兼容。
尚未落地或尚无 AX 接缝的产品面（选区指针手势与八个控制点、标注、文字、滚动）
在落地前保持 **Not tested**；落地时必须
同时按 §4.2 加 AX 标注，不得用 `--headed` 坐标点击冒充这些用例的通过证据。

## 6. 性能与资源门槛

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

## 7. 证据、隐私与放行

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

E2E 报告另外必须附：`agent-desktop --version`、`agent-desktop status` 的原始输出、
被测 `bundle_id` 与 `list-apps` 结果、执行过的确切命令序列、被引用的 stderr 诊断行
原文，以及每步出现的 `error.code`（成功步也要写 `ok`）。以 `--headed` 物理投递
作为前提的步骤要说明当时的窗口与前台状态；不得只写“点击通过”。

默认证据只保存文本、数值与 hash。除用户显式保存测试生成的 PNG 外，保存取证
图像必须另获明确授权；不得为日志、规格整理或自动化擅自保存用户截图。Safari
测试只打开本地 `file://` 页面，禁止联网。

崩溃、卡死、损坏输出、过期任务输出、意外截图落盘与运行期联网均为发布阻塞。
发布候选的必测项不能是 Fail 或 Not tested；模拟、原型和单机上的一次通过都不能
替代相应正式产品证据。
