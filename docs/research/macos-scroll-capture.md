# macOS 垂直 Scroll Capture：自动控制与拼接边界

Ticket: https://github.com/Wade11s/ArcShot/issues/4

Map: https://github.com/Wade11s/ArcShot/issues/1

研究日期：2026-10-28

范围：仅研究单一滚动区域的垂直自动捕获、权限、帧捕获、重叠检测、拼接、动态内容与失败边界；不扩展到其他产品能力，也不包含产品实现。

## 标记、方法与结论

- **[事实]**：可由 Apple 官方文档、Apple SDK 头文件/示例，或 OpenCV 官方文档/源码直接支持；均在相关句后内联引用。
- **[推断]**：由这些 API 或算法性质推导出的兼容性判断，不是平台厂商承诺。
- **[建议]**：待实现与实测验证的工程方案。文中的数值阈值均为实验初值，不是已验证的通用阈值。
- 本次“已验证”指文档、头文件、官方示例和算法源码核验；**没有**在 Safari、Chrome、Electron 或第三方 App 中做运行时兼容性实测。

> **总结论 [推断/建议]**：macOS 提供完成这条链路所需的 AX、Quartz Event、ScreenCaptureKit 与图像配准能力，但没有提供一个通用的“识别任意滚动区—滚到底—无损拼接”API。可以可靠承诺的是：**在权限、固定几何、同一像素尺度、静态内容、足够纹理和纯垂直平移等前置条件下，逐步验证并在无法验证时明确失败**。不能把任意 App 的自动识别、自动滚动、底部判断或动态页面完整性列为可靠承诺。

建议首版采用“**受控滚动 + 原生像素帧 + 有搜索范围的平移估计 + 全重叠区校验 + 仅追加新条带**”，而不是通用全景拼接。任意一步不满足判据，结果应标为失败或显式的未验证部分结果，不能静默输出貌似完整的长图。

## 1. 目标区域与滚动视图识别

### 1.1 已核实能力

- **[事实]** `AXUIElementCopyElementAtPosition` 使用左上角为基准的屏幕坐标做命中测试，并按窗口 z-order 返回最上层元素；AX 元素可读取其位置、尺寸和可执行动作等信息。[Apple AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)、[AXUIElementCopyElementAtPosition](https://developer.apple.com/documentation/applicationservices/1462077-axuielementcopyelementatposition)
- **[事实]** Apple 定义了 `AXScrollArea`、`AXScrollBar`，以及 `AXVerticalScrollBar`、`AXHorizontalScrollBar`、`AXOrientation`、`AXValue`、`AXMinValue`、`AXMaxValue` 等角色/属性；滚动条值表示滑块位置，最小/最大值可用于表达其范围。[Apple AXRoleConstants](https://developer.apple.com/documentation/applicationservices/carbon_accessibility/roles)、[kAXVerticalScrollBarAttribute](https://developer.apple.com/documentation/applicationservices/kaxverticalscrollbarattribute)、[kAXValueAttribute](https://developer.apple.com/documentation/applicationservices/kaxvalueattribute)、[kAXMaxValueAttribute](https://developer.apple.com/documentation/applicationservices/kaxmaxvalueattribute)
- **[事实]** AX 属性、动作和可写性需要逐项查询；API 明确存在 `attributeUnsupported`、`actionUnsupported`、`notImplemented`、`cannotComplete` 等失败。[AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)、[AXUIElementIsAttributeSettable](https://developer.apple.com/documentation/applicationservices/1459972-axuielementisattributesettable)、[AXUIElementCopyActionNames](https://developer.apple.com/documentation/applicationservices/1462053-axuielementcopyactionnames)
- **[推断]** 不能把某个 App 暴露的 AX 树当作所有 App 的固定接口；控制前必须做能力探测和结果验证。
- **[事实]** ScreenCaptureKit 的 `SCShareableContent` 提供可捕获的 display、window、application；`SCWindow` 暴露 `windowID`、frame、title、windowLayer 和 owningApplication。[SCShareableContent](https://developer.apple.com/documentation/screencapturekit/scshareablecontent)、[SCWindow](https://developer.apple.com/documentation/screencapturekit/scwindow)

### 1.2 工程判断与建议

- **[建议] 首选路径**：让用户点选滚动内容；从命中元素向祖先查找 `AXScrollArea`，读取其 `AXPosition`/`AXSize`、垂直滚动条及值域。用 AX 的 PID 与 `SCWindow.owningApplication.processID`、窗口几何和用户确认来对应捕获窗口。
- **[推断]** “命中元素存在 AXScrollArea 祖先”不等于该元素就是实际接收滚动的容器；嵌套滚动、自绘控件和不完整 AX 实现都会使自动识别具有试探性。AX 元素与 `SCWindow` 的自动配对也只能作为辅助，几何相同或标题为空时必须让用户确认。
- **[建议] 可靠回退**：允许用户直接框定内容矩形，随后发送一次小幅测试滚动；只有观察到目标矩形内发生预期的垂直位移，才接受该目标。自动发现失败不能阻断这种“手动区域 + 图像验证”路径。
- **[建议]** 捕获期间锁定目标逻辑身份和几何；窗口关闭、PID 改变、矩形移动/缩放、显示器或 scale 改变均中止，不能在后续帧上悄悄换目标。

**边界**：手动区域下的目标确认可纳入可靠流程；任意 App 的滚动容器自动识别只能 best-effort。

## 2. 权限：三个独立前置检查

- **[事实]** `AXIsProcessTrustedWithOptions` 检查当前进程是否为受信任的 accessibility client；传入 prompt 选项时，提示是异步的且不影响本次返回值，因此提示后仍需重新检查。[AXIsProcessTrustedWithOptions](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)
- **[事实]** Quartz 提供 `CGPreflightPostEventAccess` / `CGRequestPostEventAccess` 检查或请求事件合成权限，并分别提供事件监听权限 API；本问题所需的投递路径应检查前者，而不是以屏幕录制权限代替。[CGEvent.h / post-event access](https://developer.apple.com/documentation/coregraphics/cgevent)、[CGPreflightPostEventAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightposteventaccess())
- **[事实]** 屏幕捕获另有 `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess`。当前 ScreenCaptureKit 文档要求捕获前向用户请求 Screen Recording 权限，并说明添加 `NSScreenCaptureUsageDescription`。[CGPreflightScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightscreencaptureaccess())、[CGRequestScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess())、[ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)

**[建议]** 开始前分别 preflight AX、post-event、screen-capture；只在用户主动继续时 request，返回后重新 preflight，并实际执行一次“测试滚动 + 测试帧”。捕获中流错误、目标失效或后续权限检查失败要中止。不要把“已显示系统授权面板”当作“已获授权”。

**边界**：权限已授予和 API 调用可执行可可靠检查；用户一定授权、授权后无需 App 特定处理，均不可承诺。

## 3. 自动滚动与停止条件

### 3.1 已核实能力

- **[事实]** `CGEvent` 可构造 scroll-wheel 事件并投放到事件流；API 支持按像素或按行表示滚动单位。Apple 对 `CGEventCreateScrollWheelEvent` 明确说明：滚动通常用较小有符号整数表示，大值可能因处理事件的 App 而产生意外结果。[CGEvent](https://developer.apple.com/documentation/coregraphics/cgevent)、[CGEventCreateScrollWheelEvent](https://developer.apple.com/documentation/coregraphics/cgeventcreatescrollwheelevent)、[CGScrollEventUnit](https://developer.apple.com/documentation/coregraphics/cgscrolleventunit)、[CGEvent.post](https://developer.apple.com/documentation/coregraphics/cgevent/post(tap:))
- **[事实]** AX 可查询动作并请求元素执行动作，也可在属性可写时设置值；AX 文档说明动作调用的超时错误不一定表示动作失败。[AXUIElementPerformAction](https://developer.apple.com/documentation/applicationservices/1462091-axuielementperformaction)、[AXUIElementSetAttributeValue](https://developer.apple.com/documentation/applicationservices/1460434-axuielementsetattributevalue)
- **[推断]** 动作调用返回成功不等于已经观察到目标区域完成预期位移，因此不能用调用返回值代替图像位移验证。

### 3.2 建议控制回路

1. **[建议]** 激活并确认目标，指针位于目标内容内，先发送一个小的像素滚动作为探针。首版以 Quartz scroll event 为通用控制路径；AX 滚动条写值/动作只在对应 App 已验证时作为适配器，不作为统一假设。
2. **[建议]** 每次只推进一个逻辑步骤；为避免单个大 delta，该步骤可由多个小事件组成。随后等待新帧与稳定判据，再从相邻图像估计整个步骤的实际位移 `d`。请求的事件 delta 只用于控制，**不得直接当作拼接偏移**。
3. **[建议]** 采用闭环步长：目标为每步移动视口高度的 50%–70%，根据上一步实测 `d` 调整下一步；保留至少 25% 重叠。若一步移动达到或超过整个视口，无法验证是否漏行，应缩小步长重试或失败。
4. **[建议]** 捕获期间检测用户输入、焦点/窗口变化和惯性滚动；发现非本步骤预期的额外位移时，不继续累加，而是重新稳定、验证，无法归因则失败。

### 3.3 底部不是单一信号

- **[建议] 强判据**：目标垂直滚动条确实暴露可读的 `AXValue`/`AXMaxValue`，值已到底，且一次向下滚动后图像无正向位移。起点同理可由 `AXMinValue` 辅助确认。
- **[建议] 弱判据**：没有 AX 值时，在已有成功滚动的前提下，连续 2–3 次受控向下事件都得到相同的稳定帧，只能标为“疑似底部”。第一次滚动就无位移可能是已在底部、单屏内容、事件未送达或错误目标，不能自动等同。
- **[建议] 必须另设终止上限**：最大步数、总像素高度、总耗时和无进展重试次数；到达上限返回 `limit-reached`，不能称为完整捕获。无限列表与持续加载不能承诺存在可达底部。

**边界**：有可信 AX 值域并经图像确认的起终点可进入可靠流程；纯图像底部检测和任意无限内容只能 best-effort。

## 4. 帧捕获与坐标/尺度

- **[事实]** `SCContentFilter` 可限制为指定窗口或 display；`SCStream` 输出带附件元数据的 sample buffer。Apple 官方示例只处理 `SCFrameStatus.complete`，并读取 `displayTime`、`contentRect`、`contentScale`、`scaleFactor`。[SCContentFilter](https://developer.apple.com/documentation/screencapturekit/sccontentfilter)、[Capturing screen content in macOS（含官方源码）](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos)、[SCStreamFrameInfo](https://developer.apple.com/documentation/screencapturekit/scstreamframeinfo)
- **[事实]** `SCFrameStatus.idle` 的含义是 display 没有变化而未生成新帧；`blank`、`suspended`、`stopped` 也有独立状态。[SCFrameStatus](https://developer.apple.com/documentation/screencapturekit/scframestatus)
- **[推断]** idle 不是“已滚到底”的语义证明，因为该状态没有目标滚动位置语义。
- **[事实]** `SCStreamConfiguration` 的 width/height 是输出尺寸，`pointPixelScale` 用于 screen points 到 pixels 的换算；`sourceRect` 使用 display 逻辑坐标中的 points。当前在线文档特别注明：单窗口捕获不参考 `sourceRect`。[SCStreamConfiguration](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration)、[sourceRect](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect)、[pointPixelScale](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/pointpixelscale)
- **[事实]** 光标默认可见，`showsCursor` 可关闭；流有帧队列且官方要求 queueDepth 不要超过 8。[showsCursor](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/showscursor)、[queueDepth](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/queuedepth)
- **[推断]** 异步队列存在时，不能假设任意一个回调恰好对应当次滚动；需要按步骤关联并排除旧帧。
- **[事实]** macOS 14 起，`SCScreenshotManager` 可用 filter + configuration 捕获单帧 `CGImage` 或 sample buffer；持续 `SCStream` 则是另一条官方路径。[SCScreenshotManager](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager)、[SCStream](https://developer.apple.com/documentation/screencapturekit/scstream)
- **[事实]** ScreenCaptureKit 的错误码覆盖用户拒绝、无捕获源、无窗口列表、目标 App 连接失效/中断、系统停止流等情况，提供运行时失败检测入口。[SCStreamError.Code](https://developer.apple.com/documentation/screencapturekit/scstreamerror/code)

**[建议]** 首版用一个持续 `SCStream`，固定 SDR/BGRA、输出宽高和内容矩形，关闭光标，仅消费 `.complete` 帧；保存帧序号与元数据，拒绝滚动前的旧帧。对单窗口 filter，不依赖 `sourceRect` 裁剪子区域，而在统一输出坐标中裁剪，避免上述单窗口语义差异。macOS 14+ 可对比 `SCScreenshotManager` 的逐步单帧方案，但吞吐和时序须实测。

**[建议]** 每一步要求帧的 pixel width/height、contentRect、contentScale、scaleFactor 与基准一致；points 到 pixels 的换算集中处理。缩放、Retina 多显示器迁移、窗口 resize 或输出重采样发生后，不做跨尺度模板匹配，直接中止本次序列。

## 5. 重叠检测与拼接

### 5.1 可用算法及其实际边界

- **[事实]** Apple Vision 的 `VNTranslationalImageRegistrationRequest` 用于通过平移对齐两幅图，结果提供将 floating image 对齐到 reference image 的 `alignmentTransform`；其 observation 也继承通用 confidence。[VNTranslationalImageRegistrationRequest](https://developer.apple.com/documentation/vision/vntranslationalimageregistrationrequest)、[VNImageTranslationAlignmentObservation](https://developer.apple.com/documentation/vision/vnimagetranslationalignmentobservation)、[VNObservation](https://developer.apple.com/documentation/vision/vnobservation)
- **[事实]** OpenCV `matchTemplate` 在图像上滑动比较模板并生成完整结果图，可用 `minMaxLoc` 找最佳匹配，且支持 mask；`phaseCorrelate` 用频域方法估计平移并返回峰值附近 response，其实现要求两个输入同尺寸。[OpenCV matchTemplate](https://docs.opencv.org/4.x/df/dfb/group__imgproc__object.html)、[OpenCV phaseCorrelate](https://docs.opencv.org/4.x/d7/df3/group__imgproc__motion.html)、[OpenCV phasecorr.cpp](https://github.com/opencv/opencv/blob/4.x/modules/imgproc/src/phasecorr.cpp)
- **[事实]** OpenCV 4.x 的 `TM_CCOEFF_NORMED` 实现对零方差模板存在把结果置为全 1 的特殊分支。[OpenCV templmatch.cpp](https://github.com/opencv/opencv/blob/4.x/modules/imgproc/src/templmatch.cpp)
- **[推断]** 纯色区域不能只凭“相关系数为 1”通过验证；近纯色/低纹理区域也应先由独立纹理判据拒绝。
- **[推断]** Vision 的一个 transform/confidence 或 OpenCV 的一个最高分，都不能单独证明重叠唯一、没有重复内容、没有动态区域或拼接完整；还需搜索范围、次峰、重叠区残差和内容稳定性校验。

### 5.2 推荐的受限配准

设相邻帧均为 `W × H`，向下滚动后旧内容向上移动 `d` 个物理像素，则旧帧 `[d, H)` 应与下一帧 `[0, H-d)` 对应。

1. **[建议]** 先转固定灰度/SDR 像素表示，剔除滚动条和已确认遮罩，检查 ROI 方差与梯度能量；低纹理区域直接返回 `low-texture`，不进入“高分即成功”。
2. **[建议]** 只在 `0 < d ≤ 0.75H`、`|dx| ≤ 1px` 的范围搜索；初始目标重叠至少 `max(0.25H, 128px)`。可用受限 `matchTemplate`/NCC 找候选，也可让 Vision 提供候选 transform，但最终都要回到上述几何约束。
3. **[建议]** 同时检查：主峰分数、排除主峰邻域后的次峰差距、整个有效重叠区的绝对像素残差、残差空间分布。实验初值可设 NCC ≥ 0.99、峰差 ≥ 0.05，但必须在 fixtures 上标定并记录；不能把这些初值宣传为通用安全阈值。
4. **[建议]** 对重复列表、规则表格、长空白、近似纯色和相同卡片，若存在多个相近峰则返回 `ambiguous-overlap`；不得任选一个偏移继续。
5. **[建议]** 对 `d >= H` 返回 `no-overlap/gap`，对反向位移、显著横向位移、旋转/缩放或非平移残差返回 `unsupported-transform`。允许重试较小步长，但不能以透视 warp 或模糊接缝掩盖错误。

### 5.3 只追加确定的新内容

**[建议]** 在 `d` 和重叠区通过校验后，保留已有输出，只把当前帧底部 `d` 行（新进入视口的条带）追加到结果；重叠区不平均、不羽化，先用于验证。这样公式简单、不会把配准误差扩散成全景形变。接缝前后再做一次连续性检查，并保存每步 `d`、分数、残差和帧元数据用于诊断。

**[建议] 原生框架备选**：Vision 负责候选平移，自行完成搜索约束与全重叠验证；若希望直接分析完整匹配面和次峰，OpenCV `matchTemplate` 更贴合本方案。二者都不是“调用即保证正确”的完整 Scroll Capture。

## 6. 动态内容、固定浮层与失败边界

### 6.1 动态内容

- **[事实]** ScreenCaptureKit 提供帧状态、displayTime 和 dirtyRects 等捕获元数据；SDK 头文件将 dirtyRects 描述为重绘和移动区域的并集。[SCStreamFrameInfo](https://developer.apple.com/documentation/screencapturekit/scstreamframeinfo)
- **[推断]** dirtyRects 不是“语义内容是否变化”的分类器，只能作为变化提示。
- **[推断]** 若计时器、视频、动画、异步加载或列表重用发生在两帧之间，同一内容可能不再满足单一平移；仅匹配未变化的小块仍可能给高分，所以必须校验全部有效重叠，而非只取一个易匹配特征块。
- **[建议]** 捕获前和每步后做短时稳定性检查；将 dirtyRects 仅作为变化提示。若重叠残差呈局部动态区域、帧间几何持续变化、懒加载改变既有内容，首版应返回 `dynamic-content`。显式遮罩后的输出只能标为不含该区域的 best-effort 结果，不能声称完整。

### 6.2 固定 header/footer、悬浮按钮与滚动条

- **[建议]** 首版排除滚动条和光标；固定浮层只有在多个已移动帧中始终保持同屏坐标、边界稳定且遮罩后仍有足够重叠纹理时，才可作为 best-effort mask。该“静止即浮层”判定是启发式，不是内容语义证明。
- **[建议]** 固定元素可在结果中保留一次或按明确规则省略，但它遮住的内容不能凭拼接恢复。遮罩面积过大、浮层会变化、浮层横跨候选接缝或无法确认边界时，应失败/请求用户处理，而不是复制到每一屏或静默抹除。
- **[推断]** sticky header 与滚动内容同时出现时，若把 header 纳入配准，可能诱发错误的零位移或高残差；将其排除是降低风险，不是完整性保证。

### 6.3 必须显式失败的条件

| 失败码（建议） | 触发条件 | 允许行为 |
|---|---|---|
| `permission-denied/lost` | AX、post-event 或 screen-capture 前置/运行中失败 | 引导授权后从头重试 |
| `target-lost/geometry-changed` | 窗口/区域失效，移动、resize、显示器或 scale 改变 | 重新选择并新建序列 |
| `no-progress` | 非确认底部时滚动后无位移 | 有限重试，随后失败 |
| `no-overlap/gap` | 一步跨整屏或重叠不足 | 小步重试，否则失败 |
| `low-texture` / `ambiguous-overlap` | 无有效纹理，或多个相近匹配峰 | 调整步长；仍不唯一则失败 |
| `unsupported-transform` | 横向移动、缩放、旋转、非平移残差 | 不做自由 warp，失败 |
| `dynamic-content` | 稳定性超时或有效重叠出现不可解释变化 | 中止或显式不完整结果 |
| `capture-error/stale-frame` | 非 complete、流停止、帧序号/元数据不合法 | 重建捕获后从头或已验证检查点重试 |
| `bottom-unverified` / `limit-reached` | 底部仅弱判据，或达到资源/时间上限 | 不得标记完整 |
| `output-limit` | 预计宽高、像素数或内存超过预算 | 预分配前失败，避免部分内存峰值 |

## 7. 可承诺等级与兼容性判据

| 能力 | 可可靠承诺的范围 | 只能 best-effort / 不可承诺 |
|---|---|---|
| 权限 | 分别检查并在无权限时拒绝开始 | 用户必然授权、授权面板即成功 |
| 目标 | 用户确认矩形，且测试滚动证明事件作用于该区域 | 任意嵌套/自绘/浏览器内容的自动容器识别 |
| 控制 | 每次以实际图像位移闭环，能检测无进展和跨屏 | 任意 App 对同一 event delta 有一致响应 |
| 起终点 | 可信 AX 值域与图像共同确认 | 无 AX 时绝对证明底部、无限内容必然终止 |
| 捕获 | 固定 filter/几何/尺度下校验完整帧与元数据 | 捕获中跨 scale/resize 仍可直接拼 |
| 配准拼接 | 静态、纯垂直、足够重叠且唯一匹配时逐缝验证 | 重复/空白内容任意选峰，跨整屏补内容，自由 warp 掩盖错误 |
| 浮层/动态内容 | 能检测部分不一致并中止 | 通用语义识别、恢复被遮挡像素、保证动态页完整性 |
| 最终结果 | 所有步骤与终点判据通过才标记“已验证完成” | 部分结果自动冒充完整长图 |

**[建议] 一个目标只有同时满足以下条件才进入“兼容”集合**：

1. AX、事件投递、屏幕捕获三类检查以及目标与测试帧均通过；
2. 目标几何、输出像素尺寸和 scale 在序列中不变；
3. 每个非终端步骤都有足够重叠、唯一峰、近零横移和通过标定的全重叠残差；
4. 没有未遮罩的动态区域或未处理固定浮层；
5. 起点/终点满足上述确认规则，且资源预算内完成；
6. 任一项失败即不标“完整”。这比按 App 名称维护白名单更可解释；App/场景白名单可作为实测结果的缓存，而不是替代逐步验证。

## 8. 后续验证场景（建议）

| 类别 | 场景 | 通过标准 |
|---|---|---|
| 可控基准 | 已知唯一行号/高度的静态长页，1x/2x，短页、单屏、长页、底部半步 | 输出与参考图像逐像素/逐行一致；单屏与底部状态明确 |
| 原生控件 | AppKit `NSScrollView` 文本、表格、outline；显隐滚动条 | 目标识别、实际 `d`、AX 值域和终点均符合判据 |
| 浏览器/嵌套 | Safari 与 Chrome 的普通页面、内部滚动 div、嵌套区域、sticky header/footer | 只滚目标；浮层不满足条件时明确失败，不重复拼接 |
| 其他渲染 | 至少一个 Electron/Chromium 应用与一个自绘/虚拟列表应用 | 无可靠 AX 时可回退手动区域；内容重用导致异常时失败 |
| 纹理 | 空白页、纯色、重复卡片、规则表格、细文字、大图 | 零方差/多峰不被高分误判；无法唯一配准即失败 |
| 运动边界 | 最小/最大步长、回弹、横向滚动、惯性、快速 resize | 无漏行/重行；不支持的运动被检出 |
| 动态内容 | 视频/GIF、计时器、广告、实时更新、懒加载、无限列表 | 稳定性/残差/上限生效；不宣称完整 |
| 权限与生命周期 | 各项拒绝、授权后重试、捕获中目标关闭/最小化/失焦 | 错误分类正确，无后台错滚或输出旧目标 |
| 显示环境 | Retina/非 Retina、多显示器迁移、窗口遮挡、分辨率变化 | scale/几何变化被检出；不同时混拼 |
| 中断与资源 | 用户滚动/点击、App 卡住、超长页、内存上限 | 可取消、可诊断、在预算前失败，无静默部分成功 |

每个场景除最终图外，还应记录授权状态、目标几何、事件请求值、实测 `d`、峰/次峰、重叠残差、帧状态和结束原因；否则无法区分控制错误与拼接错误。

## 9. 本次已核实与仍属推断的事项

### 已核实（文档/头文件/源码）

- AX 命中测试、角色/属性/动作查询及错误模型；AX、事件合成、屏幕捕获是分别暴露的权限检查。[AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)、[CGEvent](https://developer.apple.com/documentation/coregraphics/cgevent)、[ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- Quartz 可投递 scroll event；ScreenCaptureKit 可捕获窗口/display、提供完整帧状态与尺度元数据，并可关闭光标。[CGEvent](https://developer.apple.com/documentation/coregraphics/cgevent)、[Apple 捕获示例](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos)
- 当前 `sourceRect` 在线文档注明了单窗口捕获限制。[sourceRect](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect)

  因而本文推断：不应把 `sourceRect` 当作所有 filter 的统一子区域接口。
- Vision 可做平移配准；OpenCV 可产生模板匹配面/相位相关结果；OpenCV 源码确有零方差模板高分陷阱。[Vision](https://developer.apple.com/documentation/vision/vntranslationalimageregistrationrequest)、[OpenCV 文档](https://docs.opencv.org/4.x/df/dfb/group__imgproc__object.html)、[OpenCV 源码](https://github.com/opencv/opencv/blob/4.x/modules/imgproc/src/templmatch.cpp)

### 推断或尚待实测

- 任意 App 的 AX 树与真实滚动容器对应质量、Quartz 事件对嵌套区域的投递表现、AX 底部值的语义一致性。
- 持续流与 `SCScreenshotManager` 在逐步滚动下的延迟、旧帧率和吞吐差异。
- 50%–70% 步长、25% 重叠、NCC/峰差/残差阈值在不同渲染与 scale 下的适用范围。
- 固定浮层自动检测的误报率，以及动态内容是否总能被全重叠残差捕获。
- 第三方 App、浏览器、Electron、HDR、多显示器和权限运行中撤销的真实失败矩阵。

## 10. 最终建议与未解决风险

**具体建议**：先做一个小型垂直捕获验证器，而不是通用 panorama：手动/AX 辅助确定一个区域，完成 AX、事件投递、屏幕捕获三类 preflight；Quartz 小步闭环滚动；ScreenCaptureKit 固定尺度捕获；每步做唯一平移与全重叠验证；只追加新条带；AX + 图像共同确认终点；所有异常显式失败。先把第 8 节的可控基准和原生控件跑通，再扩到浏览器、Electron 和动态场景。产品层面只把满足第 7 节全部判据的结果标为“已验证完成”，其余标为 best-effort 或不完整。

**未解决风险**：

1. 尚未做实机第三方 App 矩阵，无法给出具体 App 的兼容名单或成功率。
2. 没有通用语义 API 能绝对证明“底部”“内容静止”或“没有遗漏”；纯图像路径始终存在误停与漏检风险。
3. 重复/低纹理和局部动态变化可能产生高相关但错误的结果，阈值必须用参考 fixtures 和负例标定。
4. 固定浮层遮挡的原始像素不可从后续帧保证恢复，mask 后的结果天然有完整性限制。
5. 惯性、亚像素渲染、窗口/scale 改变和旧帧时序可能破坏纯平移假设，需在运行时持续监控而非只在开始前检查。
6. 超长页面和持续加载使时间、输出像素及内存成为硬边界；达到上限必须是明确失败状态。
7. 本机 Xcode 27.0 SDK 的 `SCStream.h` 仍描述 `sourceRect` 可子采样 independent-window frame，而当前在线文档注明单窗口捕获不参考它；本文采用更保守的捕获后裁剪，但实际行为仍需按最低支持 OS 版本实测。[sourceRect 在线文档](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect)、[本地 SDK 交叉核验，见 Primary sources 12](#primary-sources)

## Primary sources

1. Apple — [AXUIElement.h](https://developer.apple.com/documentation/applicationservices/axuielement_h)、[AXUIElementCopyElementAtPosition](https://developer.apple.com/documentation/applicationservices/1462077-axuielementcopyelementatposition)、[AXUIElementPerformAction](https://developer.apple.com/documentation/applicationservices/1462091-axuielementperformaction)。
2. Apple — [Accessibility Roles](https://developer.apple.com/documentation/applicationservices/carbon_accessibility/roles)、[Attributes](https://developer.apple.com/documentation/applicationservices/carbon_accessibility/attributes)、[AXIsProcessTrustedWithOptions](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)。
3. Apple — [CGEvent](https://developer.apple.com/documentation/coregraphics/cgevent)、[CGEventCreateScrollWheelEvent](https://developer.apple.com/documentation/coregraphics/cgeventcreatescrollwheelevent)、[CGScrollEventUnit](https://developer.apple.com/documentation/coregraphics/cgscrolleventunit)、[CGEvent.post](https://developer.apple.com/documentation/coregraphics/cgevent/post(tap:))、[CGPreflightPostEventAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightposteventaccess())。
4. Apple — [CGPreflightScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgpreflightscreencaptureaccess())、[CGRequestScreenCaptureAccess](https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess())。
5. Apple — [ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)、[Capturing screen content in macOS（官方示例源码）](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos)。
6. Apple — [SCShareableContent](https://developer.apple.com/documentation/screencapturekit/scshareablecontent)、[SCWindow](https://developer.apple.com/documentation/screencapturekit/scwindow)、[SCContentFilter](https://developer.apple.com/documentation/screencapturekit/sccontentfilter)。
7. Apple — [SCStream](https://developer.apple.com/documentation/screencapturekit/scstream)、[SCStreamConfiguration](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration)、[sourceRect](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/sourcerect)、[SCStreamFrameInfo](https://developer.apple.com/documentation/screencapturekit/scstreamframeinfo)、[SCFrameStatus](https://developer.apple.com/documentation/screencapturekit/scframestatus)、[SCStreamError.Code](https://developer.apple.com/documentation/screencapturekit/scstreamerror/code)。
8. Apple — [SCScreenshotManager](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager)。
9. Apple — [VNTranslationalImageRegistrationRequest](https://developer.apple.com/documentation/vision/vntranslationalimageregistrationrequest)、[VNImageTranslationAlignmentObservation](https://developer.apple.com/documentation/vision/vnimagetranslationalignmentobservation)、[VNObservation](https://developer.apple.com/documentation/vision/vnobservation)。
10. OpenCV — [Object Detection / matchTemplate](https://docs.opencv.org/4.x/df/dfb/group__imgproc__object.html)、[Motion Analysis / phaseCorrelate](https://docs.opencv.org/4.x/d7/df3/group__imgproc__motion.html)。
11. OpenCV — [templmatch.cpp](https://github.com/opencv/opencv/blob/4.x/modules/imgproc/src/templmatch.cpp)、[phasecorr.cpp](https://github.com/opencv/opencv/blob/4.x/modules/imgproc/src/phasecorr.cpp)。
12. 本地一手交叉核验 — Xcode 27.0 SDK（macOS 27.0 SDK）中的 `HIServices/Headers/AXUIElement.h`、`AXRoleConstants.h`、`AXAttributeConstants.h`、`CoreGraphics/Headers/CGEvent.h`、`CGWindow.h`、`ScreenCaptureKit/Headers/SCStream.h`、`SCScreenshotManager.h`、`SCShareableContent.h`。这些用于确认 API 注释和 availability；在线行为描述以对应 Apple 当前文档链接为准。
