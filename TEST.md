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
| MAC-START-01 | `scripts/check-startup.sh` | 正式 bundle 进入 run loop；Accessory 无 Dock 策略；真实可见 NSStatusItem、图像与菜单；`terminate:` 后 exit 0；5 秒看门狗 | 完整 MAC-START、Finder 人工视觉/点击 |
| PRIVACY-01 | `scripts/check-no-network.sh` | 正常驻留至少 0.5 秒且 `lsof -i` 无网络/监听 socket；工具缺失/检查异常非零返回，不得假通过 | 后续功能与所有时刻的网络证明 |
| BUILD-RELEASE-01 | `scripts/check-startup.sh --release` | Release bundle 同样完成驻留/退出 | 签名、公证、性能与资源门槛 |

当前实测记录见 `docs/validation/19-startup.md`。这些脚本使用正式产品二进制与
AppKit 对象；原生冒烟通过不等于完整 macOS 发布矩阵通过。

## 3. Rust 自动化接缝（随功能落地）

下表是固定测试组，不是允许用一个含大量断言的大测试代替子用例。#20 已实现无选区的权限/捕获/冻结/取消子集；具体稳定测试见下方文件。
其他选区、编辑、滚动与输出功能未实现，状态仍为 **Not tested**。

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

实机步骤与安全/预算口径见 `docs/capture.md`；实测记录见
`docs/validation/20-capture.md`。这不是 V1 其他子用例的替代。

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
- E2E 串行：同一时间只有一个代理驱动桌面。菜单命令、热键投递与 `--headed` 按键
  都会改变前台状态，并行会互相污染证据。
- 全局窗口清单（不带 `--app` 的 `list-windows`）在本机不稳定（反复
  `global_window_owner_churn`，即使 ArcShot 未运行）：一律用 `list-windows --app`，
  并且不使用需要全局清单的 `snapshot --window-id`。

### 4.2 可驱动层与观察合同

| 层 | `agent-desktop` 实测能力 | 用途 |
| --- | --- | --- |
| AppKit 状态项与 NSMenu | 状态项与四个菜单项以 role `menuitem` 暴露，`native_id` 就是 selector（`startCapture:`、`hotkeySettings:`、`screenRecordingPermissions:`、`terminate:`）；可 headless `click`，不需要打开菜单、不需要前台 | 驱动全部菜单命令 |
| 带标题栏的 GPUI 窗口（快捷键设置、权限提示） | 出现在 `list-windows --app`（尺寸/`is_focused`）；窗口内按 #53/#54 有可寻址节点：`group` 根节点带可见名称，文本是 `statictext`（值即文本），控件是 `button` 且 `native_id` 为稳定 `AXIdentifier`（`arcshot.notice.close`、`arcshot.hotkey.recorder`…） | 读窗口内容并按名字/标识驱动 |
| 无标题栏的冻结覆盖层 | 仍不出现在 `list-windows`（`titlebar: None`，非激活 borderless），但按 #55 有可寻址 AX 子树：`group "ArcShot 截图覆盖层"` + `button arcshot.overlay.cancel` | 用 `snapshot --app` 观察与取消，不靠 `--headed` Esc |

基座是 `gpui-pre` 0.3.11（ADR-0001），它上报 accessibility 子树；产品在 #53/#54/#55
给三个窗口加了 `id` + `role` + `aria_label` + `accessibility_id` + `on_a11y_action`。
E2E 的观察合同因此是**双通道**：

1. **驱动**用 AX：状态项与四个菜单项，以及窗口内的 `button` 节点（按 `native_id`
   或名字命中）。
2. **状态**以产品 stderr 诊断行为准（`capture_phase from=… to=… action=…`、
   `capture_request_us=`、`capture_probe_grid`、`capture_wake_window_submit_us=`、
   `menu_hotkey_settings`、`hotkey_register …`）。`capture_phase` 是会话相位的唯一
   真相：开始、暂停、取消、重启与失败都各有一行，不打印帧内容。冻结覆盖层必须先在
   日志中出现 `capture_wake_window_submit_us=`，取消必须出现 `to=Resident` 的相位
   行，才能声称会话真的离开了冻结态。
3. **窗口**断言只用 `list-windows --app`。驻留时它返回空列表，表示没有带标题的
   自有窗口；冻结覆盖层存在时它同样为空，因为该窗口无标题栏不被枚举。所以这个断言
   既不能证明覆盖层存在，也不能证明它已关闭，只能证明没有可捕获的自有窗口。
4. **按键**分两类。全局组合用不带 `--headed` 的 `press`（physical 投递，不要求
   ArcShot 在前台，但必须先用一个未注册组合做阴性对照）；窗口内按键（录制新组合、
   Esc）只能用 `press … --headed` 投到前台应用，并且只在产品刚报告会话或提示窗口
   打开（产品自己 `activate`）之后发出。headless `press --app` 在无焦点 AX 元素时
   必然 `ACTION_FAILED`；`--headed` 坐标点击不属于稳定断言，不能用于验收。
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
| MAC-E2E-01 | 启动被测可执行文件（stderr 重定向到日志）；`agent-desktop snapshot --surface menu-bar-extras --app "ArcShot Debug"` | 恰好一个状态项（role `menuitem`、name `ArcShot`）；其菜单四项依次为 开始截图 / 截图快捷键… / 屏幕录制权限… / 退出 ArcShot，`native_id` 与 selector 一致；不要求 ArcShot 在前台 | 图标像素观感、Dock 策略 |
| MAC-E2E-02 | 驻留时 `agent-desktop list-windows --app "ArcShot Debug"` | 返回空列表（无可捕获自有窗口） | 未标题覆盖层；见 §4.2 |
| MAC-E2E-03 | `find --surface menu-bar-extras --role menuitem --name "开始截图"` 取引用后 headless `click` | 新增 `capture_request_us=`、`capture_probe_grid` 与 `capture_wake_window_submit_us=` | 覆盖层内容、选区、输出 |
| MAC-E2E-04 | 驻留时先 `agent-desktop press <未注册组合>`，再 `press <已注册组合>`（组合从日志 `hotkey_launch config=` 翻译） | 未注册组合不产生 capture 序列；已注册组合产生与 MAC-E2E-03 相同的序列；不要求前台应用是 ArcShot | 系统冲突、冲突恢复、真实键盘布局与按键重复率 |
| MAC-E2E-05a | MAC-E2E-03 后 `agent-desktop press escape --headed`，再取 `list-windows --app` | `press` 返回 `ok` 且 `delivery=delivered_unverified`；`list-windows --app` 为空；随后开始截图仍产生新的 capture 序列 | 冻结层 NSWindow 是否被销毁（无外部探针） |
| MAC-E2E-05b | 同上，读 Esc 之后的 `capture_phase` 行 | 出现 `from=Frozen to=Resident action=CancelAttempt` | NSWindow 销毁级证据；GPUI 键处理细节（CORE-SESSION 已覆盖） |
| MAC-E2E-06 | 点击 截图快捷键… | 日志 `menu_hotkey_settings`；`list-windows --app` 出现一个可寻址窗口（`480x362`、标题 `ArcShot Debug`、`is_focused: true`） | 窗口内录入、保存、Esc 取消（无 AX 节点，策略由 APP-EFFECT 覆盖） |
| MAC-E2E-07 | 点击 屏幕录制权限… 后 `press escape --headed` | 出现 `560x282` 自有窗口；Esc 后 `list-windows --app` 为空 | 真实 TCC 提示、设置返回重检、运行中撤权 |
| MAC-E2E-08 | 点击 退出 ArcShot | 进程退出；`agent-desktop find --surface menu-bar-extras --app "ArcShot Debug"` 返回 `APP_NOT_FOUND`；`list-apps --app "ArcShot"` 为空 | 系统按 ID 重启、登录项、状态项像素残留 |
| MAC-E2E-09 | 启动前后 `agent-desktop list-apps --app "ArcShot"` | 只出现被测 `bundle_id` 与显示名（Debug/Release 各一） | 系统按 bundle ID 解析到哪一份拷贝 |
| MAC-E2E-10 | 点击 屏幕录制权限…，等 AX 建立后 `snapshot --app` | `group "ArcShot 权限提示"`、至少两个 `statictext`（标题与正文可读）、一个 `button` 带 `arcshot.notice.*` 标识；AX 激活后日志出现 `a11y_action=arcshot.notice.<id> action=Click`，窗口消失 | 未授权的变体（需改 TCC）；打开系统设置的分支不驱动 |
| MAC-E2E-11 | 点击 截图快捷键…，`snapshot --app` 后按节点驱动 | 读到当前组合与录入区名称；AX 触发录入 → 名称随状态变化（候选/不合法）→ AX 保存写入 `hotkey_store value=<新组合>`，再用同一路径恢复原组合并以 AX 关闭窗口 | 冲突恢复、暂挂失败（APP-EFFECT 覆盖） |
| MAC-E2E-12 | 开始截图后 `snapshot --app` | `group "ArcShot 截图覆盖层"` + `button arcshot.overlay.cancel`；AX 激活后 `a11y_action=arcshot.overlay.cancel action=Click` 与 `capture_phase … to=Resident`，无残留自有窗口 | 覆盖层的 `list-windows` 枚举（见 §4.5）；选区/标注（尚未实现） |

入口是 `scripts/e2e-agent-desktop.py [--release]`：它先门禁 `agent-desktop status`
与在跑实例，再用 `scripts/build-app.sh` 构建、启动被测可执行文件并把 stderr 写入
`target/e2e-agent-desktop-<configuration>.log`，然后逐条断言上表并打印
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
- 仍未落地：覆盖层的窗口级枚举。`WindowKind::PopUp` + `titlebar: None` 的非激活
  borderless 窗口在 0.3.11 下仍不出现在 `list-windows`；#55 改以 `snapshot --app`
  证明“可寻址”，NSWindow 销毁级证据仍缺外部探针。
- 已落地（#53/#54/#55）：权限提示、快捷键设置与冻结覆盖层的元素都有
  `id` + `role` + `aria_label` + `accessibility_id` + `on_a11y_action`，
  `snapshot --app` 可见、可驱动。**尚未**标注的是未来的产品面：区域选区、标注工具、
  文字编辑与滚动预览。它们落地时必须沿用 §4.2 的约定，否则 MAC-REGION /
  MAC-ANNOTATION / MAC-TEXT / MAC-SCROLL-* 仍无法进入 E2E。
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
| MAC-START | 菜单驻留、无 Dock 主窗口、全部菜单命令、默认/自定义热键、冲突恢复、偏好跨重启、真实登录项状态/失败 | 部分：菜单、默认热键、退出见 §4.3；热键录入与保存已可由 AX 驱动（MAC-E2E-11），冲突恢复与登录项仍不可驱动 |
| MAC-PERMISSION | Region Screen Recording 引导；Scroll 三权限预检、设置返回重检、运行中失效；拒启/失效不输出 | 部分：提示窗口出现/关闭与 AX 驱动见 MAC-E2E-07/10；TCC 授权与设置返回属 §4.4 |
| MAC-REGION | 单内屏框选、移动/调整、重选/撤销、完成/保存/取消、Esc、快捷键重启、2x 几何 | 否：选区尚无实现，落地时须按 §4.2 加 AX 标注才能进入 E2E |
| MAC-ANNOTATION | 全工具、命中/控制点、层叠/历史、越界、Blur、密度、悬停/焦点与贴边布局 | 否：同 MAC-REGION（工具尚未实现） |
| MAC-TEXT | 中文 IME、拉丁文字、emoji、多行；提交/Finish/保存不丢组合文字；空文本与撤销 | 否：文本编辑尚未实现；IME 候选窗另属 §4.4 |
| MAC-SCROLL-POSITIVE | 自有 GPUI 夹具、TextEdit 固定文档、Safari `file://` 静态页分别完成启动、慢速双向、反向去重、完成/取消/输出 | 未实测：目标应用侧滚动可由 `agent-desktop scroll`/`press` 投递，产品侧结果仍只能由日志证明 |
| MAC-SCROLL-UI | 右/左/选区内预览、目标内容不被浮层污染、多自有窗口全部精确排除 | 否：浮层尚未实现，落地时须补 AX 标注 |
| MAC-SCROLL-INPUT | 单帧、Enter、双击首击/完成、单击拦截、预览上滚动、前台/hit-test、Esc/重启/清理 | 否：双击/拖拽只能靠 `--headed` 坐标；单帧与 Esc 可用 AX 动作表达 |
| MAC-SCROLL-NEGATIVE | 动态、固定浮层、重复/低纹理、无重叠跳变、横向/对角/嵌套滚动；无法验证即停止 | 否：判定依赖视觉结果，需 `screenshot` 或人工（§4.4） |
| MAC-TARGET-FAIL | 目标移动/关闭/前台变化、几何/尺度变化、遮挡、输入保护/权限失效、内屏失效 | 部分：目标应用侧可用 `agent-desktop` 窗口命令制造，产品侧判定仍靠日志 |
| MAC-NATIVE-FAIL | 资源上限、SCK 错误/超时、worker 异常；普通失败与禁止输出/重启的不确定终态分离 | 否：现有 Rust/原生接缝覆盖，E2E 无法注入 |
| MAC-COLOR-OUTPUT | 命名 sRGB 全链路、PNGf 剪贴板、内存解码尺寸/像素/颜色、第三方粘贴 smoke、显式保存字节一致 | 部分：本机 `agent-desktop 1.1.0` 无剪贴板子命令，字节比对仍需脚本 |
| MAC-RECOVERY | 保存取消/失败、复制失败/重试/放弃、3 秒预览、长图整图标注、取消保留旧剪贴板 | 否：对话框尚未实现；落地后按 §4.2 加 AX 后可驱动 |
| MAC-LIFETIME-PRIVACY | 不重复输出/污染会话；回调/窗口/tap/图像引用清理；无自动截图落盘、无运行期联网 | 部分：退出清理见 MAC-E2E-08，联网见 PRIVACY-01；资源引用仍需进程级检查 |

权限切换若先触发焦点停止，只能记录实际时序，不能冒称“运行中直接撤权”已验证。
负面用例通过只表示安全处理，不外推为任意应用、动态内容、显示器或滚动速度兼容。
尚未实现的产品面（选区、标注、文字、滚动）在落地前保持 **Not tested**；落地时必须
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
