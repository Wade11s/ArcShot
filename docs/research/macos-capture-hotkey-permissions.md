# macOS 区域截图、全局快捷键与权限：纯 Rust 接口研究

Ticket: https://github.com/Wade11s/ArcShot/issues/3  
Map: https://github.com/Wade11s/ArcShot/issues/1

- 研究日期：2026-10-28（UTC）。
- 范围：只回答 ticket 的区域截图像素捕获、多显示器/Retina 坐标、全局快捷键、Screen Recording 权限、最低系统版本，以及 GPUI 覆盖层排除问题；不设计产品 UI，也不实现产品代码。
- 证据口径：Apple 当前在线文档与 Xcode 27.0 / macOS 27.0 SDK 头文件，以及 crate 官方仓库或 docs.rs 第一方文档。没有把博客、论坛经验或第三方摘要作为事实依据。
- 验证边界：本报告完成的是**文档与源码核验**，没有触发真实 TCC 授权，也没有运行 GPUI、混合 DPI 或多显示器截图测试；推断和待实测项在文末单列。

## 结论摘要

建议 ArcShot 的运行时下限设为 **macOS 14.0**，由一个很薄的 macOS 模块向其余 Rust 代码暴露以下能力：

1. `AsyncSCShareableContent` 枚举显示器和窗口，使用 display + `excludingWindows` 的 `SCContentFilter` 排除本进程全部 GPUI 窗口，再以 `AsyncSCScreenshotManager::capture_image` 获取 `CGImage`，用 `rgba_data_into` 导出 RGBA8 字节。`SCScreenshotManager` 和 `SCContentFilter.pointPixelScale` 都从 macOS 14.0 起可用；当前 `screencapturekit` 11.1.1 对这些能力提供 Rust 封装及 `async` / `macos_14_0` feature。[^A2][^A4][^R1][^R2]
2. 适配层唯一的屏幕选择矩形格式设为“**主屏左上角为原点的全局逻辑点（screen points）矩形**”，通过 `SCDisplay.displayID` 与 `CGDisplayBounds` 对接；每屏独立求交、设置局部 `sourceRect`、按该 filter 的 `pointPixelScale` 设置输出像素尺寸，最后再合成跨屏选区。Apple 明确规定 `CGDisplayBounds` 使用主屏左上角原点，`sourceRect` 使用显示器逻辑坐标系中的点，输出 `width` / `height` 使用像素。[^A4][^A5][^A6]
3. 普通全局快捷键使用 `global-hotkey` 0.9.0：其普通键路径调用 Carbon `RegisterEventHotKey`，媒体键路径才创建 `CGEventTap`；macOS manager 必须在主线程创建并运行事件循环。ArcShot 只响应 `Pressed`，保留 manager 所有权，并把注册失败呈现给用户。[^R5]
4. 用 `core-graphics` 0.25.0 的 `ScreenCaptureAccess::{preflight,request}` 封装 `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess`。`preflight` 不弹窗；`request` 只会在权限尚未决定时提示，曾被拒绝的进程不会再次自动提示，必须引导用户到“系统设置 → 隐私与安全性 → 屏幕录制”开启。[^A8][^R4]
5. “纯 Rust”按 **ArcShot 自身及调用接口只写 Rust** 理解。所选 `screencapturekit` 当前会在构建时编译自己的 Swift bridge，并要求 Rust 1.82；若要求依赖内部也不能含 Swift，则应改选 `objc2-screen-capture-kit`，接受生成的底层 API、`unsafe` 调用和自行管理 completion/lifetime 的成本。[^R1][^R6]

## 1. 捕获路径比较

| 路径 | 最低系统与能力 | 覆盖层排除 | Rust 接口与判断 |
|---|---|---|---|
| **`SCScreenshotManager.captureImage(contentFilter:configuration:)` + `screencapturekit`** | macOS 14.0；直接返回单帧 `CGImage`，当前 crate 可导出 RGBA/BGRA，并提供 executor-agnostic 的 async future。[^A2][^R2] | 支持；filter 可明确排除一组 `SCWindow`。[^A3] | **推荐**。安全高层封装覆盖本题所需的截图、filter、像素和异步接口，但依赖内部有 Swift bridge。[^R1] |
| **同一 Apple API + `objc2-screen-capture-kit` 0.3.2** | 同样是 macOS 14.0；也暴露 macOS 15.2 的区域接口。[^A2][^A7][^R6] | 支持，需自行构造同一 filter。[^A3][^R6] | **严格无 Swift bridge 时的备选**。文档中的 capture 方法是 `unsafe`，completion、对象所有权、feature 组合和像素提取都要由适配层承担，不是本题最薄的安全接口。[^R6] |
| **`SCStream` 取首帧** | ScreenCaptureKit 的 display filter 自 macOS 12.3 起可用；但本题打算使用的 `pointPixelScale` 和单帧 manager 是 14.0。[^A2][^A3][^A4] | 支持同一 filter。[^A3] | 仅在必须支持 12.3–13.x 时值得另做兼容路径；单次截图仍需 stream 生命周期、首帧状态与像素缓冲处理，不能降低当前推荐方案的总体复杂度。[^A1] |
| **`SCScreenshotManager.captureImage(in:)`** | macOS 15.2；矩形以 screen-space points 表示，明确支持跨显示器。[^A7] | **没有 content filter 参数，不能表达排除 GPUI 窗口**。[^A7] | 不单独采用；提高下限还会失去 14.0–15.1，并牺牲本题的硬性排除能力。 |
| **XCap 0.9.8 / 直接 `CGWindowListCreateImage`** | 当前 XCap 的 macOS 截图实现仍直接调用 `CGWindowListCreateImage`；当前 Apple SDK 将该函数标为 macOS 14 弃用、15 obsolete，并写明“Please use ScreenCaptureKit instead”。[^A11][^R7] | 当前 XCap 的 macOS capture helper 与 Monitor 调用只传 rect/list option/window ID，没有传 ScreenCaptureKit 的 `excludingWindows` filter。[^R7] | **不采用**。新代码不应再建立在 Apple 已要求替换的入口上。 |

### 为什么最低版本不是 12.3、15.2 或 26.0

- 12.3 是 ScreenCaptureKit/display filter 的起点，不是本题所选单帧 API 的起点；`SCScreenshotManager` 和 `pointPixelScale` 将可靠、简洁的实现推到了 **14.0**。[^A2][^A4]
- 15.2 的 `captureImage(in:)` 虽直接接受跨屏矩形，却没有 filter，无法同时满足覆盖层排除；26.0 的新 `SCScreenshotConfiguration` 也不是完成本题所必需。[^A2][^A7]
- `screencapturekit` 的 `macos_14_0` Cargo feature 需要构建机提供 macOS 14.0+ SDK；该 crate 通用下限为 Rust 1.82。[^R1]

## 2. 推荐的纯 Rust 捕获适配流程

以下是接口设计顺序，不是产品实现代码：

1. 在一次用户截图事务中重新获取 shareable content，选取 `onScreenWindowsOnly = true` 的窗口集合；显示器通过 `SCDisplay.displayID` 关联到 Quartz display ID。`SCShareableContent` 正是 displays/windows/applications 的可捕获集合，`SCWindow.windowID` 是 Core Graphics window ID。[^A1][^A9]
2. 找出 `owningApplication.processID` 等于当前 PID 的所有 `SCWindow`，将覆盖层及 ArcShot 其他本进程窗口一并排除；crate 已暴露 `owning_application()`、`process_id()` 和 builder 的 `with_excluding_windows()`。[^A9][^R3]
3. 对每个与选区相交的 `SCDisplay` 创建 `SCContentFilter::create().with_display(display).with_excluding_windows(own_windows)`。这个构造器捕获 display、桌面背景和 Dock，同时只排除指定窗口；不要用 application-excluding filter 代替，因为后者不包含桌面背景和 Dock。[^A3]
4. 设置 `sourceRect`、像素 `width` / `height`、`showsCursor = false`、`SCCaptureResolutionType::Best`，并保持输出纵横比与选区一致；然后 `await AsyncSCScreenshotManager::capture_image`。`sourceRect` 与输出尺寸的单位分别由 Apple 定义为逻辑点和像素；crate 的 async capture 返回 future。[^A4][^R2]
5. 用 `rgba_data_into` 或 `rgba_data` 导出，并以 `image.width()` / `image.height()` 的实际返回值构造结果；当前 helper 的紧密 RGBA 缓冲区大小按 `width * height * 4` 计算。[^R2]
6. 适配层对上层返回自带 `width`、`height`、`bytes_per_row`、像素格式和全局逻辑点原点的 buffer，避免调用方猜 stride 或把点当像素。同步 `capture_image` 内部会等待 completion，故 GPUI 路径优先选 `async` feature 下的 `AsyncSCScreenshotManager`，不要阻塞主事件循环。[^R2]

建议的边界类型语义如下（仅约定，不要求暴露任何 Apple 类型）：

```text
ScreenRectPoints  { origin: (f64, f64), size: (f64, f64) } // 全局、左上角、逻辑点
CaptureRequest    { selection: ScreenRectPoints, exclude_own_process: true }
CapturedImage     { rgba8: Vec<u8>, width: usize, height: usize, bytes_per_row: usize }
```

## 3. 多显示器与 Retina 坐标规则

### 3.1 只保留一个规范坐标系

- 规范选区 `G` 使用全局、左上角原点、逻辑点。`CGDisplayBounds(displayID)` 返回相对主屏左上角的 display bounds，正好提供每屏在同一空间中的 `D`。[^A5]
- `SCDisplay.width` / `height` 是 **points，不是 pixels**。当前 crate 的 `display.rs` 注释写成了 pixels，应以 Apple 文档和 SDK 头文件为准，不能照该注释做 Retina 换算。[^A6][^R3]
- 若输入来自 AppKit 的全局 bottom-left `NSRect`，先转换成上述规范矩形。Apple 的 Cocoa 坐标说明使用左下角原点，而 Quartz display bounds 使用主屏左上角原点；令 `H = CGDisplayBounds(CGMainDisplayID()).height`，则 `R_top = (x, H - y - height, width, height)`。这是坐标系之间的代数转换，不涉及缩放。[^A5][^A10]
- 不使用 `NSScreen.visibleFrame` 代替 display bounds：Apple 明确 `frame` 才包含菜单栏和 Dock 占据的完整区域。[^A10]

### 3.2 每屏求交和点 → 像素

对每个满足 `I = intersection(G, D)` 且非空的显示器：

```text
L       = (I.x - D.x, I.y - D.y, I.width, I.height)  // display-local points
s       = filter.pointPixelScale                     // 当前 filter 的 points -> pixels
sourceRect = L
outW    = round(I.width  * s)
outH    = round(I.height * s)
```

Apple 将 `sourceRect` 定义为“display’s logical coordinate system”中的点，将 `pointPixelScale` 定义为 screen points 到 pixels 的换算因子，并将配置输出宽高定义为 pixels，因此这里不需要猜测 Retina 是否恒为 2。[^A4]

捕获后的像素 `(u, v)` 若要映射回全局逻辑点，应使用实际图片尺寸抵消取整误差：

```text
sx = image.width  / I.width
sy = image.height / I.height
p  = (I.x + (u + 0.5) / sx, I.y + (v + 0.5) / sy)
```

AppKit 的 `backingScaleFactor` 也表示每线性 screen-space unit 对应多少 backing pixels，但 Apple 建议 AppKit 视图尽量使用 backing conversion 方法；本捕获路径优先使用与 content filter 绑定的 `pointPixelScale`，不要跨显示器复用一个 scale。[^A4][^A10]

### 3.3 跨屏选区的合成是显式策略，不是隐藏假设

在 macOS 14.0 推荐路径上，每个 display filter 产生一张局部图片。若产品需要一张跨屏 RGBA 图，选择合成比例 `S`（建议选区起点所在显示器的 `pointPixelScale`），创建 `round(G.width * S) × round(G.height * S)` 画布，将每块放在 `round((I.origin - G.origin) * S)`，并按各块实际像素尺寸缩放到 `round(I.size * S)`。[^A4]

混合 DPI 时，一张规则像素网格无法同时保持所有显示器的“一个物理像素 = 一个输出像素”；上述策略保留选区锚点显示器的比例，其他屏可能发生重采样。若后续要求所有屏都绝不重采样，应保留每屏 fragment 及其逻辑坐标，而不是宣称存在单一无损位图。这属于由 API 单位推导出的合成决策，并非 Apple 对 ArcShot 的既有行为保证。

## 4. GPUI 覆盖层不入镜

**已核验的系统能力：** `SCContentFilter.init(display:excludingWindows:)` 捕获 display 时排除指定 `SCWindow`；`SCWindow` 提供 `windowID` 和可选 `owningApplication`，`SCRunningApplication` 提供 PID。[^A3][^A9]

**推荐的 GPUI 适配：**覆盖层已经显示后，再枚举 shareable content，以当前 PID 找出本进程所有 `SCWindow` 并加入每屏 filter。这样不要求 GPUI 把底层 `NSWindow` 暴露给截图模块，也不会因只匹配标题而漏掉无标题 panel；PID、window ID 和 builder 方法在 Rust crate 中均已有接口。[^R3]

**必须保留的失败路径：**如果覆盖层应当可见却一个本进程 `SCWindow` 也找不到，不应静默继续并把覆盖层拍进去；应重取一次内容或返回可诊断错误。macOS 14.4+ 另有 `getCurrentProcessShareableContent`，它返回当前进程无需 TCC 即可获取的、经 redaction 的 shareable content，但它不能替代全屏捕获所需的 Screen Recording 授权，因此只作为以后优化，不抬高本题 14.0 下限。[^A12]

这里的“覆盖层先显示再枚举”和“找不到则失败”是基于 API 语义的适配建议；GPUI 实际使用的 window/panel 是否在各种 Space、Stage Manager 状态下都进入该集合，仍须实机验证。

## 5. 全局快捷键

### 推荐接口

- `global-hotkey` 0.9.0，Rust 1.77；macOS 上 manager 必须在主线程创建，主线程必须有事件循环。[^R5]
- 普通快捷键注册走 `RegisterEventHotKey`，事件通过 Carbon application event target 送回，再转成带 `Pressed` / `Released` 状态的 `GlobalHotKeyEvent`；Drop 会注销已注册热键。[^R5]
- ArcShot 只在 `Pressed` 时启动一次截图事务，忽略 `Released`；保留 manager，提供 unregister，并把 `FailedToRegister` 直接映射为“快捷键不可用/请更换”，不能注册失败后仍显示为已启用。[^R5]

### 权限边界与冲突

普通键分支没有创建 `CGEventTap`，所以本方案不为普通全局快捷键增加 Accessibility 或 Input Monitoring 的前置授权。当前 crate 的媒体键分支才创建 system-defined event tap；Apple 对键盘事件 tap 的监听设有 assistive/access 检查。不要把媒体键所需的事件监听权限误加到 ArcShot 的普通快捷键上。[^A13][^R5]

crate 调用 `RegisterEventHotKey` 时 options 为 0，而非 `kEventHotKeyExclusive`。Apple 当前 SDK 说明非独占 hotkey 可被多个应用注册并分别收到通知，所以不能向用户承诺“注册成功即系统独占”；保留可配置快捷键和冲突提示。[^A14][^R5]

`CGEventTap` / 全局键盘监听路径虽然能做更灵活的过滤，却会引入额外权限和 run-loop/tap 生命周期，不适合作为本题普通截图快捷键的首选。[^A13][^R5]

## 6. Screen Recording 权限检测与引导

### 系统事实

- Apple 要求全屏捕获前取得 Screen Recording 权限，并在 app 的 Info.plist 配置 `NSScreenCaptureUsageDescription`，说明用途。[^A1]
- `CGPreflightScreenCaptureAccess()` 查询当前进程是否已有权限且不提示；`CGRequestScreenCaptureAccess()` 在状态未决定时请求并提示。此前被拒绝的进程不会自动再次提示，只能由用户在 Screen Recording 设置中开启；两者均自 macOS 10.15 起可用。[^A8]
- `core-graphics` 0.25.0 已将这两个 C 函数封装为 `ScreenCaptureAccess::preflight()` / `request()`，因此 ArcShot 不必再写一段裸 FFI。[^R4]
- Apple 的 macOS capture sample 说明首次运行会请求 Screen Recording，授权后需要重启 sample app 才能捕获；因此适配层要允许“重试”和“重启后再试”，不能假定设置变化一定立即反映到已运行进程。[^A1]

### 推荐状态流

```text
用户点击截图
  → preflight()
    → true：执行捕获
    → false：先显示 ArcShot 自己的用途说明
        → 用户确认后 request()
        → 提供“打开设置后重试”
        → 仍 false：引导 系统设置 → 隐私与安全性 → 屏幕录制
        → 授权后重试 preflight；仍未生效则提示重启 ArcShot
```

`request()` 的返回值只表示调用时当前进程是否已有 access，不应把一次 `false` 当成永久拒绝。这两个函数的 Bool 结果没有提供独立的 `notDetermined` / `denied` 状态枚举，UI 不应声称能精确区分二者；首次 `false` 可提供 `request()`，请求后仍为 `false` 则统一提供设置页引导与重试。Apple 头文件另已明确“曾被拒绝则不重新提示”。[^A8]

捕获调用仍可能返回错误；`SCStreamError.Code.userDeclined` 的官方含义是用户没有授予 Screen Recording。当前高层 crate 的截图错误主要经字符串错误通道传递，因此不要把解析错误字符串当作权限状态的权威来源；启动前使用 preflight，失败后可再 preflight 一次区分权限变化与其他捕获错误。[^A15][^R2]

不要把普通快捷键的 Accessibility/Input Monitoring 与 Screen Recording 捆绑成“全部权限”：本题推荐路径只需要为**像素捕获**申请 Screen Recording；普通 Carbon hotkey 注册不走 event-tap 权限路径。[^A8][^A13][^R5]

## 7. 已核验与推断清单

### 已由 Apple 文档/SDK 或第一方 crate 源码核验

- API 名称、macOS 14.0 / 14.4 / 15.2 的 availability，filter 排除窗口、display logical points、`pointPixelScale`、输出 pixels，以及 `CGDisplayBounds` 原点。[^A2][^A3][^A4][^A5][^A7][^A12]
- `CGWindowListCreateImage` 当前弃用状态，Screen Recording preflight/request 的提示与拒绝语义，普通 Carbon hotkey 和 event-tap 的不同代码路径。[^A8][^A11][^A13][^A14][^R5]
- 本报告列出的 crate 版本、feature、主线程要求、同步/异步截图、RGBA helper 和 Swift bridge 构建事实。[^R1][^R2][^R4][^R5][^R6][^R7]

### 推断/设计决策（不是已实测的系统保证）

- 将跨屏选区拆成每屏局部 `sourceRect` 后按锚点屏 scale 合成，是依据 Apple 单位定义提出的适配算法；取整、旋转显示器和混合 DPI 的视觉结果未实测。
- 以 PID 排除所有 ArcShot 窗口可避免依赖 GPUI 私有窗口细节，但 GPUI panel 的 shareable-content 枚举时序未实测。
- 普通 hotkey 无需额外权限是由 crate 的普通键实现不创建 event tap 得出的接口边界；具体快捷键组合在当前 macOS 上是否可用仍由注册结果决定。

## 8. 来源清单

[^A1]: Apple，[ScreenCaptureKit framework](https://developer.apple.com/documentation/screencapturekit)；[Capturing screen content in macOS](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos)（权限、purpose string、授权后重启 sample、stream 配置）。
[^A2]: Apple，[SCScreenshotManager](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager) 与 [captureImage(contentFilter:configuration:completionHandler:)](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/captureimage(contentfilter:configuration:completionhandler:))；Xcode 27 SDK `ScreenCaptureKit.framework/Headers/SCScreenshotManager.h`（macOS 14.0 availability、CGImage 返回）。
[^A3]: Apple，[SCContentFilter.init(display:excludingWindows:)](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(display:excludingwindows:))；Xcode 27 SDK `SCStream.h` 第 102–180 行附近（macOS 12.3、desktop/Dock 包含关系、不同 filter 语义）。
[^A4]: Apple，[SCStreamConfiguration.sourceRect](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect)、[SCContentFilter.pointPixelScale](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/pointpixelscale)、[contentRect](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/contentrect)、[SCCaptureResolutionType](https://developer.apple.com/documentation/screencapturekit/sccaptureresolutiontype)；Xcode 27 SDK `SCStream.h`（sourceRect 为 display logical points、输出 width/height 为 pixels、scale/captureResolution availability）。
[^A5]: Apple，[CGDisplayBounds(_:)](https://developer.apple.com/documentation/coregraphics/cgdisplaybounds(_:))（全局 display coordinate space，主屏左上角原点）。
[^A6]: Apple，[SCDisplay](https://developer.apple.com/documentation/screencapturekit/scdisplay)（displayID、frame、width/height 均为 points）；Xcode 27 SDK `SCShareableContent.h` 第 95–116 行附近。
[^A7]: Apple，[SCScreenshotManager.captureImage(in:completionHandler:)](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/captureimage(in:completionhandler:))；Xcode 27 SDK `SCScreenshotManager.h`（macOS 15.2、points、跨显示器、无 filter 参数）。
[^A8]: Apple，[CGPreflightScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightscreencaptureaccess()) 与 [CGRequestScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess())；Xcode 27 SDK `CoreGraphics.framework/Headers/CGWindow.h` 第 294–308 行（不提示、未决定才提示、拒绝后不重新提示、macOS 10.15）。
[^A9]: Apple，[SCShareableContent](https://developer.apple.com/documentation/screencapturekit/scshareablecontent)、[SCWindow](https://developer.apple.com/documentation/screencapturekit/scwindow)、[SCWindow.windowID](https://developer.apple.com/documentation/screencapturekit/scwindow/windowid)、[SCRunningApplication.processID](https://developer.apple.com/documentation/screencapturekit/scrunningapplication/processid)。
[^A10]: Apple，[NSScreen.frame](https://developer.apple.com/documentation/appkit/nsscreen/frame)、[NSScreen.backingScaleFactor](https://developer.apple.com/documentation/appkit/nsscreen/backingscalefactor)、[Coordinate Systems and Transforms](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaDrawingGuide/Transforms/Transforms.html)、[NSWindow.convertToBacking(_:)](https://developer.apple.com/documentation/appkit/nswindow/converttobacking(_:))。
[^A11]: Apple，[CGWindowListCreateImage](https://developer.apple.com/documentation/coregraphics/cgwindowlistcreateimage)；Xcode 27 SDK `CoreGraphics.framework/Headers/CGWindow.h` 第 223–283 行（macOS 14 deprecated / 15 obsolete，以及“Please use ScreenCaptureKit instead”）。
[^A12]: Apple，[SCShareableContent.getCurrentProcessShareableContent](https://developer.apple.com/documentation/screencapturekit/scshareablecontent/getcurrentprocessshareablecontent(completionhandler:))；Xcode 27 SDK `SCShareableContent.h` 第 154–159 行（macOS 14.4、当前进程、redacted、无需 TCC）。
[^A13]: Apple，[CGEvent.tapCreate](https://developer.apple.com/documentation/coregraphics/cgevent/tapcreate(tap:place:options:eventsofinterest:callback:userinfo:))、[CGPreflightListenEventAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightlisteneventaccess())；Xcode 27 SDK `CoreGraphics.framework/Headers/CGEvent.h` 第 250–300、398–408 行（键盘 tap 的 access 检查与监听权限入口）。
[^A14]: Apple，Xcode 27 SDK `Carbon.framework/Frameworks/HIToolbox.framework/Headers/CarbonEvents.h` 第 15380–15525 行（`RegisterEventHotKey`、options=0 的非独占语义、虚拟键码与 modifiers）；[Apple 64-bit Carbon Event Manager 说明](https://developer.apple.com/library/archive/documentation/Carbon/Conceptual/Carbon64BitGuide/HIToolboxChanges/HIToolboxChanges.html)。
[^A15]: Apple，[SCStreamError.Code.userDeclined](https://developer.apple.com/documentation/screencapturekit/scstreamerror/code/userdeclined)。
[^R1]: `screencapturekit` 第一方仓库固定版本 [`673ba10`](https://github.com/doom-fish/screencapturekit-rs/tree/673ba10e79c53022eaf597bcb1c762f22e6096be)：[Cargo.toml](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/Cargo.toml)（11.1.1、Rust 1.82、features）与 [build.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/build.rs)（Swift bridge、macOS 14 SDK 检查）。
[^R2]: `screencapturekit` 第一方源码固定版本：[screenshot_manager.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/src/screenshot_manager.rs)（同步等待、CGImage、RGBA/BGRA 与紧密 stride）和 [async_api.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/src/async_api.rs)（`AsyncSCScreenshotManager`）。
[^R3]: `screencapturekit` 第一方源码固定版本：[content_filter.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/src/stream/content_filter.rs)、[window.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/src/shareable_content/window.rs)、[running_application.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/src/shareable_content/running_application.rs)、[display.rs](https://github.com/doom-fish/screencapturekit-rs/blob/673ba10e79c53022eaf597bcb1c762f22e6096be/src/shareable_content/display.rs)（builder、PID 接口；display.rs 的 pixels 注释与 Apple 不符）。
[^R4]: `core-graphics` 第一方仓库固定版本 [`88cb72c`](https://github.com/servo/core-foundation-rs/tree/88cb72cb2e0ac3f1e4f39e04d80f0b24f8374401)：[access.rs](https://github.com/servo/core-foundation-rs/blob/88cb72cb2e0ac3f1e4f39e04d80f0b24f8374401/core-graphics/src/access.rs)、[display.rs](https://github.com/servo/core-foundation-rs/blob/88cb72cb2e0ac3f1e4f39e04d80f0b24f8374401/core-graphics/src/display.rs)（`CGDisplay::bounds`）、[Cargo.toml](https://github.com/servo/core-foundation-rs/blob/88cb72cb2e0ac3f1e4f39e04d80f0b24f8374401/core-graphics/Cargo.toml)；[docs.rs ScreenCaptureAccess 0.25.0](https://docs.rs/core-graphics/0.25.0/core_graphics/access/struct.ScreenCaptureAccess.html)。
[^R5]: `global-hotkey` 第一方仓库固定版本 [`c0ee632`](https://github.com/tauri-apps/global-hotkey/tree/c0ee632d8dd4725f2c299cb010453590baa6ecec)：[Cargo.toml](https://github.com/tauri-apps/global-hotkey/blob/c0ee632d8dd4725f2c299cb010453590baa6ecec/Cargo.toml)（0.9.0、Rust 1.77）、[lib.rs](https://github.com/tauri-apps/global-hotkey/blob/c0ee632d8dd4725f2c299cb010453590baa6ecec/src/lib.rs)（主线程/event loop、事件）、[macos/mod.rs](https://github.com/tauri-apps/global-hotkey/blob/c0ee632d8dd4725f2c299cb010453590baa6ecec/src/platform_impl/macos/mod.rs)（普通键 Carbon、媒体键 event tap、options=0、Drop）、[error.rs](https://github.com/tauri-apps/global-hotkey/blob/c0ee632d8dd4725f2c299cb010453590baa6ecec/src/error.rs)。
[^R6]: `objc2-screen-capture-kit` 0.3.2 第一方生成文档：[SCScreenshotManager](https://docs.rs/objc2-screen-capture-kit/0.3.2/objc2_screen_capture_kit/struct.SCScreenshotManager.html)（`unsafe` capture 方法、completion 与 CGImage/CMSampleBuffer）；[crate](https://docs.rs/crate/objc2-screen-capture-kit/0.3.2)。
[^R7]: XCap 第一方仓库固定版本 [`5c205f2`](https://github.com/nashaofu/xcap/tree/5c205f20cde2d5bdfcbc058db073cfd274a8eeda)：[macos/capture.rs](https://github.com/nashaofu/xcap/blob/5c205f20cde2d5bdfcbc058db073cfd274a8eeda/src/macos/capture.rs) 与 [macos/impl_monitor.rs](https://github.com/nashaofu/xcap/blob/5c205f20cde2d5bdfcbc058db073cfd274a8eeda/src/macos/impl_monitor.rs)。

## 9. 最终建议

采用 **macOS 14.0+ / Rust 1.82+** 的单一主路径：

- 捕获：`screencapturekit = "11"`，启用 `async,macos_14_0`，`AsyncSCShareableContent` → display + 本进程 `excludingWindows` filter → `AsyncSCScreenshotManager` → RGBA8。[^R1][^R2][^R3]
- 坐标：适配层只接收全局左上角逻辑点；每屏求交、局部 `sourceRect`、filter scale、实际图片尺寸回传；跨屏合成比例显式记录。[^A4][^A5]
- 快捷键：`global-hotkey = "0.9"`，主线程创建，普通 Carbon hotkey，只处理 `Pressed`，注册失败可见。[^R5]
- 权限：`core-graphics = "0.25"` 的 preflight/request + Info.plist purpose string + 设置页人工引导/重试/必要时重启；普通 hotkey 不附带 Accessibility 或 Input Monitoring 请求。[^A1][^A8][^R4][^R5]
- 如果“纯 Rust”被进一步解释为**依赖中禁止 Swift bridge**，不要悄悄接受当前高层 crate：改用 `objc2-screen-capture-kit` 实现相同 Apple 调用图，并把 unsafe/completion/像素代码封进 macOS 模块。[^R1][^R6]

## 10. 未决风险

1. **实机矩阵尚未验证：**至少补测 macOS 14 与当前系统、单屏 Retina/非 Retina、左右及上下负坐标排列、混合 DPI、旋转屏、跨屏选区；重点检查取整后 1 px 偏差和合成比例。
2. **GPUI 窗口枚举时序：**需验证 overlay 创建后立即查询时，所有 Space/Stage Manager 状态下的 GPUI window/panel 都能以当前 PID 出现在 `SCShareableContent`；找不到时必须失败而非入镜。
3. **TCC 生命周期：**授权、拒绝、运行中撤销、开发期不同启动宿主，以及签名/打包后身份变化都需测试；preflight 与实际捕获之间仍存在权限变化竞态。
4. **快捷键组合：**非独占注册不代表独占可用，系统保留组合或其他应用策略可能影响事件；需要实测默认组合并保留用户改键与失败提示。
5. **构建链定义：**`screencapturekit` 的 Swift bridge 是否符合项目对“纯 Rust”的验收口径必须确认；同时需锁定 macOS 14+ SDK 的构建环境和 crate 兼容版本。
6. **本报告未做运行验证：**以上风险不会被文档引用自动消除；它们应成为实现后的验收用例，而不是继续扩大本次研究范围。
