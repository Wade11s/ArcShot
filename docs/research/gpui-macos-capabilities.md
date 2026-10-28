# GPUI 在 macOS 上构建 ArcShot 的能力边界

Ticket: https://github.com/Wade11s/ArcShot/issues/2

Map: https://github.com/Wade11s/ArcShot/issues/1

- 调研日期：2026-10-28（UTC）。
- 范围：仅回答票据中的应用生命周期、菜单栏、多显示器透明覆盖层、鼠标/键盘、文本输入与 IME、图像渲染、窗口层级与焦点；不评估或扩展其他产品能力。
- 主要源码基线：`zed-industries/zed@37faef81f74fc1bb08be9939c4cffc8ede6ced2f`。该快照中的 `gpui` manifest 标为 `0.2.2`，`gpui_macos` 标为 `0.1.0`；本文链接均固定到提交，而非漂移的 `main`。[S1][S2]
- 平台绑定基线：`madsmtm/objc2@8a5abacd90d5403fc0463459a35826f892b9142e`；Zed 同一快照的 workspace manifest 将 `objc2-app-kit` 标为 `0.3.2`、将 `objc2-core-graphics` 固定为 `=0.3.2`。[S3][S4][S28]

## 结论摘要

**推荐采用“GPUI 主界面与窗口 + 小型纯 Rust macOS 适配层”，而不是纯 GPUI，也不是自建整套原生 UI。** GPUI 已直接覆盖 Accessory 生命周期、每显示器窗口、透明背景、窗口内鼠标/键盘、IME 协议桥接和图像显示；菜单栏右侧状态项、全局快捷键必须补充平台代码，任意窗口层级、点击穿透及精确的前台应用恢复则保留为按需适配点。[S5][S6][S7][S8][S9][S10][S11][S12][S14][S18][S22]

下表中的“已核实”指**公共 API 与 macOS 后端实现已在上述源码快照中核对**，不等于已在 ArcShot 或实体多显示器上运行验收。

| 能力 | GPUI 直接提供 | 纯 Rust macOS 适配 | 判断与成熟度 |
|---|---|---|---|
| 无 Dock、无常驻主窗口的生命周期 | `ActivationPolicy::Accessory`、启动前 `with_windowing(WindowingRequest::Headless)`、`QuitMode::Explicit`、`open_window` / `remove_window` / `quit`。Accessory 明确允许继续开窗口。[S5][S6][S13] | 状态项需要被原生对象图保留，见下一行。 | **可直接实现，源码已核实。** 官方有 headless/window 切换示例，但没有 ArcShot 这种“始终 Accessory + 状态项”的完整示例；冷启动行为需实测。[S13] |
| 应用主菜单与菜单栏右侧状态项 | `App::set_menus` 和 `Menu` / `MenuItem` 只对应应用菜单；官方 `set_menus` 示例演示这一路径。[S7] | **`NSStatusBar` / `NSStatusItem` 适配不可省略。** `objc2-app-kit` 提供对应 feature 和绑定；Apple 将状态项定义为系统菜单栏中的独立元素。[S3][S14] | **两种“菜单”必须分开。** GPUI 公共 API 中本次未找到状态项抽象；状态项是明确适配边界。 |
| 多显示器无边框透明覆盖层 | `displays()`、`DisplayId`、`WindowOptions::display_id`、窗口相对显示器的 bounds、`WindowBackgroundAppearance::Transparent`；`observe_displays` 报告增删和变化。[S8][S9][S15] | 仅当需要 GPUI 未暴露的特殊原生行为时补充。 | **基本覆盖层可直接实现，未做实机验收。** `titlebar: None` 在 macOS 后端是隐藏标题栏的 `Titled \| FullSizeContentView`，并非字面上的 `Borderless` style mask。[S9] |
| 覆盖层层级与 Spaces | `WindowKind::Floating` 使用 floating level；`PopUp` 使用 popup level、`NonactivatingPanel`、`CanJoinAllSpaces \| FullScreenAuxiliary`。[S9] | 任意 `NSWindow.Level`、额外 collection behavior 或点击穿透需通过原生窗口补充。[S9][S16] | **普通截图覆盖层优先试 `PopUp`；高层级/全屏应用兼容性仍是风险。** 源码证明设置了这些属性，不证明能压过所有第三方窗口。 |
| 窗口内鼠标与键盘 | `MouseDownEvent` / `MouseMoveEvent` / `MouseUpEvent` / `ScrollEvent`、`KeyDownEvent` / `KeyUpEvent`、hitbox、focus、`KeyBinding` / action 分发。[S10][S17] | 无。 | **直接提供，源码和官方示例已核实。** 拖拽、Escape 等属于应用事件逻辑，不是平台适配。 |
| 全局快捷键 | 本次核对的 GPUI key dispatch 以窗口内 focus/key context 为边界，未发现系统全局 hotkey 注册 API。[S17] | **必须适配。** 可先以 AppKit `NSEvent.addGlobalMonitorForEvents` 接收其他应用的事件副本；需要拦截/消费时再评估 `CGEventTap`。[S18][S19] | **明确适配边界。** 全局监听不能等同于 GPUI 窗口快捷键；权限、冲突和安全输入场景必须实测。 |
| 文本输入与 IME | `EntityInputHandler` / `ElementInputHandler` 覆盖文本、UTF-16 selection、marked text、替换、候选框 bounds；macOS view 实现 `NSTextInputClient` 并接入 `inputContext`。[S11][S20] | 不需要为 IME 协议另写 AppKit 适配。 | **协议桥接直接提供，但编辑控件不是零代码。** 官方 `input` 示例手工实现文本状态、选择、绘制和 UTF-16 转换，ArcShot 也需实现或复用这层应用逻辑。[S11] |
| 图像渲染 | `img`、`ImageSource::Render(Arc<RenderImage>)`、资源/内存图加载与 Metal 后端；`RenderImage` 可由 `image::Frame` 构造。[S12][S21] | 截图采集本身不在本票据范围；把已取得的图像放进 GPUI 不需要原生 view。 | **直接提供。** 原始帧到 `RenderImage` 的集成是由 API 得出的实现推断，Retina 尺寸、色彩和显存仍需用真实帧验证。 |
| 基本窗口焦点控制 | 创建时 `focus` / `show`，运行时 `App::activate` / `Window::activate_window`，另有 active 状态查询。[S6][S9][S15] | 若必须记录并恢复之前的前台应用，使用 `NSRunningApplication`；若需隐藏而不销毁、非激活行为或任意层级，也进入适配层。[S16][S22] | **基本控制直接提供；跨应用焦点恢复不宜假设 GPUI 会自动完成。** 多窗口同时显示时仍只有一个 key window，键盘/IME 目标必须明确。[S27] |

## 分项依据与边界

### 1. 无 Dock、无主窗口的应用生命周期

GPUI 的 `ActivationPolicy::Accessory` 文档语义是“不参与 Dock/菜单栏等前台应用 UI，但保留打开窗口的能力”；`Application::with_windowing` 可在 `run` 前设置初始 windowing 模式，macOS 后端会在启动 run loop 前对 Accessory 调用 `setActivationPolicy`，避免启动时先注册到 Dock。[S5][S6] 因此推荐**启动前设置 Accessory**，不要等窗口出现后再从 Regular 切换，后者可能带来 Dock 图标闪现，且 GPUI 文档提醒 Regular 后还需自行 activate。[S6]

`QuitMode::Explicit` 使最后窗口关闭不自动退出，配合显式 `cx.quit()` 可让常驻状态项进程在 Capture Session 开关之间存活；macOS 的默认 quit mode 本身也是 Explicit，但显式写出更符合本应用语义。[S5] 官方 `toggle_headless` 示例同时展示了 `with_windowing`、`with_quit_mode(QuitMode::Explicit)`、动态开窗和 `on_window_closed`；它不是菜单栏状态项示例，不能据此证明状态项已经由 GPUI 管理。[S13]

**边界：** GPUI 管进程、窗口和激活策略；ArcShot 仍需让自己的状态项对象及 session 状态在无窗口期间存活。原生 AppKit 初始化应在 GPUI `Application::run` 的启动回调中执行；`objc2-app-kit` 明确要求大量 AppKit 功能须等 application delegate 完成 launching 后使用。[S6][S23]

### 2. 菜单栏集成：应用菜单 ≠ 状态项

`App::set_menus` 替换的是应用 menu bar，`set_dock_menu` 则是 Dock 图标右键菜单；二者都不创建系统菜单栏右侧的状态项。[S7] 本报告对 `gpui` 与 `gpui_macos` 的公开平台 API、`NSStatusBar` / `NSStatusItem` 标识做了检索，未发现 GPUI 状态项封装；这是一项**对指定源码快照的检索结论**，不是对所有第三方扩展的否定。

纯 Rust 适配可直接使用 `objc2-app-kit` 的 `NSStatusBar`、`NSStatusBarButton`、`NSStatusItem` feature。[S3] Apple 的一手文档提供 `NSStatusBar.statusItem(withLength:)`、`NSStatusItem.button` 和 `NSStatusItem.menu`；当前文档把旧的 `image`、`title` 等属性列入 Deprecated，因此新代码应优先走 button/menu，而不是复制旧式状态栏示例。[S14]

### 3. 多显示器透明覆盖层

`App::displays()` 返回活动显示器，`PlatformDisplay` 提供 ID、稳定 UUID、bounds 和 visible bounds；`WindowOptions::display_id` 选择目标屏，`WindowBounds::Windowed` 接受像素 bounds。[S8][S15] macOS 后端把每个 display 的 bounds 表示为相对该屏的矩形，并在开窗时把该矩形换算到目标 `NSScreen`；所以“一屏一个窗口、每屏从 `(0,0)` 到 `display.bounds().size`”有直接源码依据，不应把另一屏的绝对原点重复叠加进去。[S9][S15]

透明窗口由 `WindowBackgroundAppearance::Transparent` 选择；后端会更新 renderer transparency、`NSWindow.opaque` 和背景色。[S9][S24] `titlebar: None` 则隐藏标题和标题栏并使用 full-size content view，示例也以此创建 custom-titlebar 窗口。[S9][S25] 因而可以实现视觉上的无边框透明窗口，但报告不把这种实现误写成 GPUI 已暴露 `NSWindowStyleMask.Borderless` 选项。

显示器热插拔和属性变化可通过 `App::observe_displays` 观察；事件包括 Added、Removed、Changed。[S8] GPUI 文档同时说明，被移除显示器上的窗口会由平台移动到其他显示器，因此 session 需要在变化后重新核对“窗口—显示器—图像”的映射，而不能只依赖初始枚举。[S8]

### 4. 鼠标、键盘与全局快捷键

GPUI 的事件结构包含窗口坐标、按钮、modifiers 和 click count；元素 API 提供 mouse down/move/up、scroll、key down/up、capture/bubble 和 action 处理。[S10][S17] 这足以表达覆盖层内的框选与键盘命令，但**快捷键只到达当前 GPUI 窗口/focus 分发链**，不能据此实现其他应用处于前台时的全局唤醒。

`NSEvent.addGlobalMonitorForEvents(matching:handler:)` 的 Apple 定义是安装一个接收“系统投递给其他应用的事件副本”的 monitor，适合作为不消费原事件的唤醒路径。[S18] 如果需要拦截并不再投递快捷键，才需评估 Quartz event tap；Apple 将 `CGEvent.tapCreate` 定义为创建 event tap，且键盘 event tap 涉及额外权限条件。[S19] 两条路径都不是 GPUI API，均需 Rust 平台代码、回调生命周期管理和到 GPUI 主线程的消息转发。

### 5. 文本输入与 IME

`EntityInputHandler` 要求组件提供文本范围、selection、marked range、文本替换、marked-text 替换以及 range bounds；`Window::handle_input` 在绘制时把 `ElementInputHandler` 与 focus 关联。[S11] macOS 后端为 view 添加 `NSTextInputClient`，实现 `hasMarkedText`、`markedRange`、`selectedRange`、`insertText`、`setMarkedText`、`unmarkText` 和 `firstRectForCharacterRange`，并在组成中文/日文等输入时把事件送入 `inputContext`。[S20]

结论是：**不需要另写 IME 平台桥，但需要认真实现组件。** 官方 `input` 示例本身包含 marked range、UTF-16 与 UTF-8 偏移转换、候选位置、鼠标选择和绘制，说明 GPUI 提供的是输入协议与文本系统，而不是一行配置即可使用的完整文本框。[S11] 对 ArcShot 的文字标注，中文/日文/韩文、emoji、组合字符、候选框在透明 `PopUp` 中的位置都需按真实组件验证。

### 6. 图像渲染

`ImageSource` 同时接受资源、`Arc<Image>`、自定义 loader 和已经解码的 `Arc<RenderImage>`；`img(source)` 是正式元素 API。[S12] `RenderImage::new` 接收 `image::Frame` 集合并保留帧尺寸，官方 image/image_loading 示例覆盖 PNG/JPEG/SVG/GIF、文件资源与异步加载路径；macOS 使用 Metal，启用 `font-kit` 才能避免占位文本系统。[S12][S21]

因此截图帧显示不需要另建 `NSImageView` 或 Metal renderer。这里需要区分：API 已证明能显示 `RenderImage`，但**具体采集帧的 RGBA/BGRA 顺序、预乘 alpha、物理像素与逻辑点换算、Display P3 和释放策略尚未在本项目中验证**；这些属于接入验证，不应误称为 GPUI 不支持图像。

### 7. 窗口层级与焦点

GPUI macOS 后端把 `Normal`、`Floating`、`PopUp` 分别映射到 normal、floating、popup level；`PopUp` 还使用 non-activating panel 和跨 Spaces collection behavior。[S9] 创建参数可分别控制 `focus` 与 `show`：二者皆真时走 `makeKeyAndOrderFront`，仅 show 时走 `orderFront`；运行时可调用 `activate_window`。[S9][S15]

这些 API 足以构造“所有屏显示、仅一个交互窗口接收键盘”的候选方案，但当前源码没有公共的任意 level 参数，也没有 macOS 点击穿透开关。需要时，`Window` 实现 `HasWindowHandle`，macOS 后端返回 AppKit `NSView` 指针；Apple 的 `NSView.window` 可取得所属 `NSWindow`，从而在**小而明确的 unsafe 适配层**设置 `NSWindow.Level` 或其他属性。[S16][S22][S26] 这条路径会修改 GPUI 所有的原生窗口，必须保持 GPUI 所有权、主线程和生命周期约束，不宜成为默认 UI 架构。

`App::activate` 只激活当前应用；GPUI 公共 API 并未承诺在 session 结束时恢复此前前台应用。Apple 的 `NSRunningApplication.activate(options:)` 提供激活另一运行中应用的一手平台路径，所以若实测关闭覆盖层不能得到所需焦点恢复，应把“记录旧前台应用并恢复”列入同一适配层，而不是再开一套窗口系统。[S6][S22]

## 推荐技术路径（具体顺序）

1. **固定依赖版本/提交。** GPUI README 明确说明仍在 active development、pre-1.0 且版本间常有 breaking changes；不要跟随未固定的 `*` 或漂移的 `main` 做架构承诺。[S21]
2. **以 GPUI 为唯一窗口与渲染框架。** 启动前设 `WindowingRequest::Headless`（macOS 映射 Accessory）和 `QuitMode::Explicit`，无常驻隐藏主窗口；session 结束移除覆盖窗口，进程仅在显式退出时结束。[S5][S6]
3. **建立窄 `macos_bridge`，先只做两件必需事项：**
   - 在 GPUI 完成 launching 后创建并持有 `NSStatusItem`，使用 button/menu API，把 action 转成应用命令。[S3][S14][S23]
   - 用 `NSEvent` global monitor 做第一版可配置全局唤醒；GPUI 窗口内仍使用 `KeyBinding` / action。只有在“必须消费快捷键”被证实为需求时才引入 event tap。[S17][S18][S19]
4. **每个 Capture Session 重新枚举显示器，每屏开一个 GPUI 窗口。** 以 `display_id` + 相对该屏的完整 `display.bounds` 设置 `WindowBounds::Windowed`，使用 `titlebar: None`、`WindowKind::PopUp`、`WindowBackgroundAppearance::Transparent`、不可移动/缩放；先让所有窗口 show，只给实际键盘/IME 目标窗口 focus，其他窗口不主动 focus。[S8][S9][S15]
5. **输入、IME 和图像留在 GPUI。** 用 GPUI mouse/key/action 做覆盖层交互；按官方 `input` 示例实现 `EntityInputHandler` 与候选框 bounds；真实截图帧解码/转换为 `Arc<RenderImage>` 后用 `img` 显示。[S10][S11][S12]
6. **把额外原生窗口行为设为验证后的增量，而非前置重写。** 仅当 `PopUp` 不能满足层级/全屏兼容、需要点击穿透或必须恢复旧前台应用时，经 `HasWindowHandle → NSView → NSWindow` 和 `NSRunningApplication` 增补适配。[S16][S22]

## 已验证与未验证

### 已由 primary sources 验证

- 上述 GPUI 类型、方法和枚举确实存在于固定源码快照，且 macOS 后端对 Accessory、目标显示器、透明背景、窗口 kind/level/Spaces、focus/show、IME 协议和 raw AppKit view handle 有实际实现，不只是文档中的跨平台占位。[S5][S6][S9][S15][S20][S24][S26]
- `objc2-app-kit` / `objc2-core-graphics` 的第一方绑定 manifest 确实提供状态项、窗口、事件和 CGEvent 等对应 feature；Apple 一手文档确认相应原生 API 的职责。[S3][S4][S14][S16][S18][S19]

### 基于源码的实现推断（尚未实机证明）

- `Accessory + Explicit` 与自持有状态项可以组成长期无窗口驻留进程；每屏一个透明 `PopUp` 可以满足 ArcShot 的基本覆盖层。
- `NSEvent` global monitor 的非消费语义适合作为第一版全局唤醒；单一 focused overlay 可以承载键盘和 IME，同时其他屏继续响应鼠标。
- 原始截图帧经 `RenderImage` 接入后无需额外原生渲染代码；跨应用焦点恢复可在需要时安全封装进适配层。

### 必须保留的验证项与风险

1. **GPUI 演进风险：高。** pre-1.0 breaking changes 是官方明示；升级必须重跑本报告所列 API/行为验证。[S21]
2. **冷启动与打包风险：高。** 需从 Finder 启动正式 `.app`，验证无 Dock 图标、无主窗口、状态项可点击、打开首个窗口和显式退出；不能只从 terminal 运行判断。GPUI 后端自己对 unbundled app 的菜单时序有特殊处理，说明启动方式确实可能影响行为。[S6]
3. **多显示器/Spaces 风险：高。** 需覆盖不同缩放、负坐标排列、刘海屏、热插拔、睡眠唤醒、Mission Control 和其他应用原生 fullscreen；`PopUp` 的源码属性不保证压过所有同级或更高层级窗口。[S8][S9]
4. **焦点与 IME 风险：高。** 需验证 non-activating popup 中键盘、中文/日文/韩文 marked text、候选框位置、emoji/组合字符及 session 结束后的旧应用焦点；多窗口只有一个键盘目标是本方案必须显式管理的不变量。[S27]
5. **全局快捷键风险：高。** 需测试权限授予/撤销、安全输入、快捷键冲突、其他应用占用和回调失效；global monitor 只提供副本，若要求消费原事件就必须重新评估 event tap 与权限成本。[S18][S19]
6. **真实帧渲染风险：中。** 需检查 Retina 下尺寸、通道顺序、alpha、色域、深色/浅色外观、超大图内存和多窗口纹理释放；源码支持图像不等于这些接入细节已验收。[S12][S21]
7. **原生适配安全风险：中。** `NSStatusItem`、全局 monitor、raw `NSView`/`NSWindow` 都有主线程、retain/release、回调注销和 GPUI 所有权约束；必须把 unsafe 与生命周期管理收敛在窄适配层，不能散落到 UI 组件。[S16][S23][S26]

## Primary sources

- **[S1]** Zed，GPUI crate manifest（版本）：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/Cargo.toml#L1-L9
- **[S2]** Zed，`gpui_macos` manifest（版本及一方平台依赖）：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/Cargo.toml#L1-L55
- **[S3]** objc2，`objc2-app-kit` 绑定 features（`NSStatusBar` / `NSStatusItem`）：https://github.com/madsmtm/objc2/blob/8a5abacd90d5403fc0463459a35826f892b9142e/framework-crates/objc2-app-kit/Cargo.toml#L1706-L1719
- **[S4]** objc2，`objc2-core-graphics` 绑定 features（`CGEvent` / `CGWindowLevel`）：https://github.com/madsmtm/objc2/blob/8a5abacd90d5403fc0463459a35826f892b9142e/framework-crates/objc2-core-graphics/Cargo.toml#L54-L100
- **[S5]** GPUI，ActivationPolicy 与 QuitMode：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L157-L172 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L491-L500
- **[S6]** GPUI，启动 windowing、activation policy 与 macOS 启动时序：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L256-L271 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L2063-L2079 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/platform.rs#L531-L550
- **[S7]** GPUI，应用菜单 API 与官方 `set_menus` 示例：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L2870-L2884 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/set_menus.rs
- **[S8]** GPUI，display API 与 display changes：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L122-L140 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L636-L674 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L1728-L1737
- **[S9]** GPUI macOS，窗口创建、目标屏、隐藏标题栏、level、Spaces 与 focus/show：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L970-L1037 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L1142-L1160 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L1184-L1215 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L1264-L1269
- **[S10]** GPUI，鼠标/键盘事件结构与元素处理：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/interactive.rs#L22-L52 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/interactive.rs#L148-L167 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/interactive.rs#L489-L550 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/elements/div.rs#L1000-L1220
- **[S11]** GPUI，EntityInputHandler 与官方 IME/input 示例：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/input.rs#L7-L114 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/input.rs#L288-L440 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/input.rs#L600-L619
- **[S12]** GPUI，ImageSource、img 与 RenderImage：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/elements/img.rs#L44-L119 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/elements/img.rs#L178-L216 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/assets.rs#L42-L103
- **[S13]** GPUI 官方 `toggle_headless` 示例：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/toggle_headless.rs
- **[S14]** Apple，NSStatusBar / NSStatusItem 一手文档：https://developer.apple.com/documentation/appkit/nsstatusbar ；https://developer.apple.com/documentation/appkit/nsstatusbar/statusitem(withlength:) ；https://developer.apple.com/documentation/appkit/nsstatusitem ；https://developer.apple.com/documentation/appkit/nsstatusitem/button ；https://developer.apple.com/documentation/appkit/nsstatusitem/menu
- **[S15]** GPUI，WindowOptions / WindowBounds 与 displays / activate：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L2599-L2674 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L2743-L2765 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L1604-L1637 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/window.rs#L6696-L6700
- **[S16]** Apple，NSWindow.Level 与 NSView.window：https://developer.apple.com/documentation/appkit/nswindow/level ；https://developer.apple.com/documentation/appkit/nsview/window
- **[S17]** GPUI 官方 Key Dispatch 文档：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/docs/key_dispatch.md
- **[S18]** Apple，`NSEvent.addGlobalMonitorForEvents(matching:handler:)`：https://developer.apple.com/documentation/appkit/nsevent/addglobalmonitorforevents(matching:handler:)
- **[S19]** Apple，Quartz Event Services / `CGEvent.tapCreate`：https://developer.apple.com/documentation/coregraphics/quartz-event-services ；https://developer.apple.com/documentation/coregraphics/cgevent/tapcreate(tap:place:options:eventsofinterest:callback:userinfo:)
- **[S20]** GPUI macOS，NSTextInputClient 与 IME 事件路径：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L210-L255 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L2660-L2785 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L3369-L3490
- **[S21]** GPUI README（pre-1.0、macOS Metal/font-kit、源码为重要文档）：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/README.md#L1-L70 ；官方图像示例：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/image/image.rs ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/image_loading.rs
- **[S22]** Apple，`NSRunningApplication.activate(options:)`：https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(options:)
- **[S23]** objc2-app-kit 初始化与 NSWindow 所有权说明：https://github.com/madsmtm/objc2/blob/8a5abacd90d5403fc0463459a35826f892b9142e/framework-crates/objc2-app-kit/src/lib.rs#L1-L25
- **[S24]** GPUI macOS，透明背景实现：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L1832-L1875
- **[S25]** GPUI 官方窗口示例（custom titlebar、window kinds、focus/show）：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/window.rs
- **[S26]** GPUI，Window 与 macOS 后端的 raw AppKit handle：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/window.rs#L7644-L7655 ；https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L2390-L2405
- **[S27]** Apple，`NSApplication.keyWindow`（接收键盘事件的窗口）：https://developer.apple.com/documentation/appkit/nsapplication/keywindow
- **[S28]** Zed workspace，objc2 绑定版本：https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/Cargo.toml#L738-L773

[S1]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/Cargo.toml#L1-L9
[S2]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/Cargo.toml#L1-L55
[S3]: https://github.com/madsmtm/objc2/blob/8a5abacd90d5403fc0463459a35826f892b9142e/framework-crates/objc2-app-kit/Cargo.toml#L1706-L1719
[S4]: https://github.com/madsmtm/objc2/blob/8a5abacd90d5403fc0463459a35826f892b9142e/framework-crates/objc2-core-graphics/Cargo.toml#L54-L100
[S5]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L157-L172
[S6]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L2063-L2079
[S7]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/app.rs#L2870-L2884
[S8]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L636-L674
[S9]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L970-L1037
[S10]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/interactive.rs#L22-L52
[S11]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/input.rs#L7-L114
[S12]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/elements/img.rs#L44-L119
[S13]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/toggle_headless.rs
[S14]: https://developer.apple.com/documentation/appkit/nsstatusbar
[S15]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/platform.rs#L2599-L2674
[S16]: https://developer.apple.com/documentation/appkit/nswindow/level
[S17]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/docs/key_dispatch.md
[S18]: https://developer.apple.com/documentation/appkit/nsevent/addglobalmonitorforevents(matching:handler:)
[S19]: https://developer.apple.com/documentation/coregraphics/quartz-event-services
[S20]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L210-L255
[S21]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/README.md#L1-L70
[S22]: https://developer.apple.com/documentation/appkit/nsrunningapplication/activate(options:)
[S23]: https://github.com/madsmtm/objc2/blob/8a5abacd90d5403fc0463459a35826f892b9142e/framework-crates/objc2-app-kit/src/lib.rs#L1-L25
[S24]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui_macos/src/window.rs#L1832-L1875
[S25]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/examples/window.rs
[S26]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/crates/gpui/src/window.rs#L7644-L7655
[S27]: https://developer.apple.com/documentation/appkit/nsapplication/keywindow
[S28]: https://github.com/zed-industries/zed/blob/37faef81f74fc1bb08be9939c4cffc8ede6ced2f/Cargo.toml#L738-L773
