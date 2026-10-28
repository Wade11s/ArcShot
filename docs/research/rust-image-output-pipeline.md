# 纯 Rust 标注合成、马赛克与 PNG 剪贴板输出研究

Ticket: https://github.com/Wade11s/ArcShot/issues/5
Map: https://github.com/Wade11s/ArcShot/issues/1

研究日期：2026-10-28。研究范围只覆盖 Ticket 的 Question：原始截图与可编辑标注的最终合成、矩形高亮、线类标注、Mosaic Rectangle、文本栅格化、Retina 像素尺寸、颜色/alpha、超长图、PNG 另存为及 macOS `NSPasteboard` 输出；不扩展产品功能，也不交付产品代码。

## 读法与结论摘要

- **[事实]** 表示已在所列第一方文档、源码、Apple 文档或 PNG 规范中直接核对的 API、约束或行为。
- **[建议]** 表示基于这些事实为 ArcShot 推荐的架构/语义，不是某个 crate 已经提供的完整产品实现。
- **[推断/未验证]** 表示由源码推导的影响或尚未实测的事项。此次没有编写产品代码，也没有进行 macOS 实机粘贴、GPUI 像素对比或长图性能/峰值内存 benchmark。

**建议定案：**最终输出采用独立的 CPU 合成器，而不是截取 GPUI 预览：

> 原始截图（明确的 RGBA/色彩契约）→ 尺寸与内存检查 → `tiny-skia::Pixmap`（预乘 RGBA）→ Mosaic Rectangle → 矩形/路径描边 → `cosmic-text` 文字 → `take_demultiplied` → `png::Encoder` 编码一次为 `Vec<u8>` → 同一份字节用于 `NSPasteboardTypePNG` 或另存为。

其中 `tiny-skia` 负责几何光栅化，`cosmic-text` 负责 shaping/layout/字体回退与文字栅格化，`png` 负责纯 Rust PNG 编码，`objc2-app-kit`/`objc2-foundation` 只承担 macOS 系统边界。GPUI 继续负责交互预览，但预览和最终输出共享标注模型、坐标变换、字体资源及颜色约定。

## 1. 已核验的事实

### 1.1 Crate 能力及“纯 Rust”的边界

- **[事实]** `tiny-skia` 官方 README 将其描述为移植到 Rust 的 Skia 子集，目标是 CPU-only 2D 渲染，支持实体/渐变/图案填充与描边、裁剪、图像混合和 PNG 读写；同一 README 明确列出文字渲染和 ICC profiles 不在其能力范围内。[S1] `Pixmap` 的文档进一步说明它拥有**预乘（premultiplied）RGBA**像素，且数据宽度等于 stride。[S2]
- **[事实]** `cosmic-text` 0.19.0 的第一方文档将能力列为 shaping、字体发现、字体回退、布局、栅格化和编辑；其 shaping 使用 `harfrust`，字体发现使用 `fontdb`，可选栅格化使用 `swash`。[S7] 这比单纯逐字符取 glyph 的栅格器更适合需要换行、Unicode shaping 或回退的文本标注。
- **[事实]** `png` 官方仓库明确说其 PNG decoder/encoder 是纯 Rust，且仓库说明没有 `unsafe` 代码。[S11] `objc2` 官方项目则是 Apple frameworks 的 Rust bindings；因此“纯 Rust 管线”应理解为图像、文字和 PNG 不依赖 C/C++ 图像库，**不意味着 macOS 剪贴板可以绕过 Objective-C/AppKit 系统接口**。[S18]
- **[事实]** `cosmic-text` 0.19.0 的默认 feature 包含 `std`、`swash` 和 `fontconfig`；其 `std` feature 还启用字体数据库和相关 Rust 组件。[S10] 为严格审计依赖并提高输出可复现性，建议实现时关闭默认 feature，只开启经过审查的 `std`/`swash`，用显式字体集；不能把默认 feature 的依赖情况当成已完成的全平台纯 Rust 证明。[建议]

### 1.2 GPUI 预览与 Retina 物理像素

- **[事实]** Apple 将 `NSScreen.backingScaleFactor` 定义为每个屏幕空间线性单位对应的 backing-store 像素数；`NSWindow.backingScaleFactor` 在高分辨率缩放模式为 `2.0`、其他情况为 `1.0`，但 Apple 同时建议通常使用 backing 坐标转换，而不是把该属性当作布局/物理尺寸。[S21] `NSView.convertToBacking(_:)` 的文档明确把点或尺寸转换到像素对齐的 backing-store 坐标系。[S22]
- **[事实]** GPUI 文档把 `DevicePixels` 定义为显示器上的实际物理像素，把 `ScaledPixels` 定义为考虑设备 scale factor 的长度；`Pixels.scale(factor)` 会产生 `ScaledPixels`。[S23] 这支持在预览层区分逻辑长度和设备长度，但不意味着 GPUI 的预览位图就是最终 PNG。
- **[建议]** `CaptureBitmap` 必须携带实际的 `width_px`、`height_px` 和坐标/颜色约定；最终 PNG 的宽高直接使用这些物理像素值。可编辑标注可保存逻辑坐标，但合成时只做一次“逻辑坐标 → 捕获 backing 像素”的变换，线宽、文字字号和 mosaic block size 也在同一边界转换，禁止在 PNG 或剪贴板阶段再次乘 Retina scale。多屏/滚动拼接若存在不同 scale，必须保存可确定的统一输出映射，不能凭当前窗口 scale 猜测。

### 1.3 几何合成、预乘 alpha 与颜色

- **[事实]** `tiny-skia::Paint` 的默认混合模式是 `SourceOver`，默认开启抗锯齿；`Stroke` 有浮点 `width`、`line_cap`、`line_join` 和 dash 属性。[S4][S5] 因而它可以直接覆盖可调线宽的线、折线和箭头头部路径，而不必把粗线拆成大量单像素 Bresenham 线。[建议]
- **[事实]** `tiny-skia::ColorSpace` 的默认值为 `Linear`，另有 `Gamma2`、`SimpleSRGB` 和 `FullSRGBGamma`；`FullSRGBGamma` 应用完整 sRGB gamma 函数，但不进行 CIE XYZ 转换。[S6] PNG 规范则要求 alpha 是线性不透明度、RGB 不以预乘形式保存，并给出应在 intensity samples 上进行合成的说明。[S15]
- **[事实]** `tiny-skia::Pixmap::from_vec` 接收的 buffer 必须是预乘 RGBA；其 PNG 解码实现会逐像素把 RGB 乘以 alpha 后再构造 Pixmap。[S2][S3] `take_demultiplied` 会消费 Pixmap、原地反预乘并返回 RGBA 字节。[S3] PNG 的 truecolor-with-alpha（color type 6）是 RGB 后接 alpha，且 PNG 使用非预乘 alpha。[S14][S15]
- **[建议]** 输入契约必须明确原始截图是 straight 还是 premultiplied RGBA，进入 Pixmap 前只预乘一次，输出前只反预乘一次；不能把已经预乘的 capture bytes 再当 straight bytes 处理。若输入已归一化为 sRGB，几何 `Paint` 统一选 `FullSRGBGamma`，并用 1x/2x、透明/半透明 golden images 验证与 GPUI 的视觉差异；若输入是 Display-P3/HDR 或带其他 ICC，不能直接声称经过这条管线后色彩仍正确。
- **[建议]** `Color` 的标注 API 使用非预乘 RGBA8，透明度只由合成层处理。Mosaic 是替换式像素滤波而不是 `SourceOver` 涂一层颜色；其块平均也应遵循同一色彩策略（建议颜色在线性光中累加、alpha 保持线性），不能悄悄把存储字节的简单平均当成经过验证的色彩正确性。

### 1.4 文本栅格化的真实边界

- **[事实]** `cosmic-text` 的 `Metrics` 以像素表示 `font_size` 和 `line_height`；`Buffer` 负责 shaped/laid-out text，`SwashCache` 负责用 swash 栅格化，`Buffer::draw` 可以把 glyph/装饰输出为带位置和颜色的回调。[S8] `FontSystem::new` 使用已安装字体，`new_with_fonts` 则接受预先指定的字体集。[S8]
- **[事实]** `cosmic-text` 的 `SwashContent` 区分 8-bit alpha `Mask`、32-bit RGBA `SubpixelMask` 和 32-bit RGBA `Color`。[S9] 0.19.0 `SwashCache::with_pixels` 的 Mask 分支把 coverage byte 放进回调颜色的 alpha，同时只保留 base color 的 RGB；该表达式不包含 base alpha。[S9] 同一源码的 `SubpixelMask` 分支目前只记录 TODO 警告。[S9]
- **[建议]** 最终文字 pass 使用物理像素的 `Metrics` 和 glyph 位置；Mask coverage 要与标注颜色 alpha 相乘后，再转换成 `tiny-skia` 的非预乘 `Color` 并交给它做一次预乘/SourceOver。彩色 glyph 与 Mask 分开处理，不能把回调字节 blindly 当作已预乘像素；V1 采用灰度 Mask 路径，除非另行实现并测试 SubpixelMask。预览和输出必须共享字体文件、字体回退配置、字号/行高和 scale。
- **[事实/备选]** `ab_glyph` 的第一方 API 覆盖 OpenType glyph 的加载、缩放、定位和栅格化，`OutlinedGlyph::draw` 回调提供 coverage。[S28] `imageproc` 0.27.0 的 `draw_text_mut` 虽可直接使用 `ab_glyph`，其实现按 `text.chars()` 逐字取 glyph、应用 advance/kerning，文档明确不支持换行。[S25] 因此它是简单固定字体场景的轻量备选，不应被当作完整 shaping/fallback 方案。

### 1.5 PNG 编码与超长图限制

- **[事实]** `png::Encoder::new` 接收 `u32` 宽高；`set_color(ColorType::Rgba)` 和 `set_depth(BitDepth::Eight)` 分别选择 RGBA 和每样本 8 位，`Writer::write_image_data` 写入图像数据，`Writer::finish` 做结束校验。[S12][S13] `png` 也提供 `StreamWriter`，文档称可创建无法放入内存的图像；这只是编码端的流式写出能力，并不会自动把全尺寸 `Pixmap` 合成变成分块合成。[S13]
- **[事实]** `tiny-skia::Pixmap::new` 对零尺寸报错，文档注明宽度受 `i32::MAX/4` 限制；其实现还用 checked arithmetic 计算行字节数和总数据长度。[S2][S3] 由于 `tiny-skia` 每像素固定 4 字节，单个 Pixmap 的原始像素存储下界为 `4 × width_px × height_px` 字节（还未计 PNG 输出、原始截图保留副本、字体缓存和 allocator overhead）。[S3]
- **[事实]** `Pixmap::encode_png` 的 0.12.0 实现先 `to_owned()` 复制整张图，再反预乘并写入 PNG；相比之下，`take_demultiplied()` 可消费并复用 Pixmap 自己的 `Vec<u8>`。[S3] 所以“消费 Pixmap 后交给 `png::Encoder`”是减少一次整图复制的**源码推导建议**，不是已经测得的性能提升。[推断]
- **[事实]** W3C 的 PNG chunk 规范页面记载宽高为四字节整数、零无效，并将每维上限写为 `2^31-1`；这不是 ArcShot 应该允许分配的内存预算。[S14] `image` crate 另有面向解码的 `Limits`，其文档明确称为 decoding resource limits，不能把它当作本管线输出端的自动保护。[S29]
- **[建议]** 在分配前用 `u64`/`usize` checked multiplication 检查 `4 × W × H`，同时检查 crate 尺寸约束和可配置的输出预算；超出预算返回可诊断错误，不静默缩小截图。预算必须包括保留的原始图、合成图、PNG `Vec<u8>`、可能的 `NSData` 复制以及文字缓存。超长图若超过全帧预算，需要另做分块/分条合成与流式编码设计，不能把 `StreamWriter` 当作现成解决方案。

### 1.6 macOS PNG pasteboard 与另存为

- **[事实]** Apple 文档把 `NSPasteboard.general` 称为通用内容共享 pasteboard，并给出先 `clearContents` 再写数据的模式；`clearContents` 是提供新内容前清除旧内容的第一步。[S16][S17] `setData(_:forType:)` 将数据写为第一个 item 的指定表示，返回成功/失败的 `Bool`；`NSPasteboard.PasteboardType.png` 的含义就是 PNG image data。[S17]
- **[事实]** `objc2-app-kit` 0.3.2 对应提供 `generalPasteboard()`、`clearContents()` 和 `setData_forType(Option<&NSData>, &NSPasteboardType) -> bool`；`NSPasteboardTypePNG` 是该 crate 的静态 PNG 类型绑定。[S18][S20] `objc2-foundation` 的 `NSData` 提供 `with_bytes(&[u8])` 和（在相应 feature 下）`from_vec(Vec<u8>)` 构造方式。[S19]
- **[建议]** 先在 Rust 内成功完成合成和 PNG 编码，再调用通用 pasteboard：清空旧内容，把同一份 PNG bytes 包装为 `NSData`，以 `NSPasteboardTypePNG` 调用 `setData_forType`，检查返回值并处理系统通信错误。不要写入原始 RGBA 却声明成 PNG，也不要为了复制而先经过 `NSImage`/TIFF 再重新编码。另存为复用同一个 `Vec<u8>` 写入用户选择的 `.png` 文件，不重新光栅化、不再缩放；这样复制与保存的像素和编码结果一致。
- **[建议]** `NSPasteboard`/`NSData` 的生命周期和线程边界集中在一个 macOS adapter 中，并保持 PNG bytes/`NSData` 至少活到写入调用返回。`objc2` 的 `NSPasteboard` 文档标为 `!Send`/`!Sync`，PNG 静态类型绑定也标为 `unsafe static`，这些平台细节不应泄漏到纯 Rust 合成器。[S18][S20]

## 2. 为什么不把 `imageproc` 作为首选几何层

`image + imageproc` 是可行的轻量备选，但其默认语义需要更多适配：

- **[事实]** `imageproc` 的普通 `Canvas` 对 `GenericImage` 的 `draw_pixel` 等同于 `put_pixel`，即覆盖；只有显式使用 `Blend` 才会先调用 `Pixel::blend` 再写回。[S24] 因此半透明矩形/线若直接画到 `RgbaImage`，会把背景替换掉，而不是完成高亮合成。[事实]
- **[事实]** `draw_line_segment_mut` 的签名只有起终点和颜色，源码逐点走 Bresenham iterator，没有粗线宽度参数。[S26] 粗线、圆角连接和箭头需要另写几何/抗锯齿逻辑，或改用支持 Stroke 的库。[推断]
- **[事实]** `imageproc` 文档说其函数默认假定像素颜色位于线性 RGB，而 `draw_text_mut` 自己已经按 glyph coverage 混合像素。[S25][S27] 若给文本再套 `Blend`，从调用顺序看会再次混合，故不能对所有图元机械使用同一个 Canvas。[推断]
- **[事实]** `image` 0.25.10 默认 feature 包含 `default-formats` 和 `rayon`，`imageproc` 0.27.0 默认还启用 `image/default`、`text`、`fft`。[S29][S30] 如果未来改选此备选，应关闭不需要的默认 feature、只保留 PNG/text 并审计锁文件；这也是依赖和二进制体积的额外工作。[建议]

**取舍结论：**在只需要极简单固定线宽图形时，`imageproc` 可减少依赖；但本 Ticket 同时要求可调线类标注、透明度、文字和 Retina 输出，`tiny-skia` 的 Stroke/抗锯齿与统一预乘模型更少手写风险，故选它作为几何层。[建议]

## 3. 推荐落地管线（设计，不是已交付实现）

### 3.1 输入、顺序与输出契约

1. **CaptureBitmap：**保存非零物理宽高、RGBA8 channel 顺序、straight/premultiplied 标志、颜色空间和捕获坐标映射；原始图与可编辑标注分离，撤销/重绘不依赖已破坏的 PNG。
2. **AnnotationDocument：**保存逻辑坐标、逻辑线宽/字号/block size、颜色和有序标注列表。最终合成器只接收不可变快照，不读取 GPUI view 的 GPU 内容。
3. **尺寸/预算检查：**先检查非零、整数转换、`4 × W × H` 和总额外副本预算，再分配 Pixmap；任何溢出或超预算都失败并保留用户可诊断状态。
4. **归一化并合成：**按输入标志进行至多一次预乘，构造 `Pixmap`。建议先应用所有针对原始截图的 Mosaic Rectangle，再按文档顺序画高亮/线/箭头/文字；若未来允许 mosaic 覆盖其他标注，必须把顺序规则写成文档并加测试。
5. **编码一次：**消费 `Pixmap::take_demultiplied()`，用 `png::Encoder` 以 RGBA8 写出到一个 `Vec<u8>`，调用结束校验并传播错误。小图可直接使用 `Pixmap::encode_png`，但长图主路径不采用其整图复制实现。
6. **分发同一份结果：**默认复制时走 `NSPasteboardTypePNG`；显式另存为时写同一 `Vec<u8>`。输出不再经过 GPUI、`NSImage` 或第二次尺寸变换。

### 3.2 各标注类型的合成规则

- **矩形高亮：**物理像素矩形先裁剪到图像边界，使用带 alpha 的 `Paint` + `fill_rect`；边框用同一 Paint 的 `stroke_path`，不要先 `put_pixel` 覆盖背景。
- **线/折线/箭头：**把逻辑点转换到物理像素后用 `PathBuilder`，以物理 `Stroke.width`、明确的 cap/join 和抗锯齿绘制；箭头头部作为填充路径。零长度、越界、非有限浮点和裁剪后空路径要在适配层处理。
- **Mosaic Rectangle：**将矩形和 block size 转到物理像素，裁剪后以实际边缘块宽/高分组；对每块所有源像素计算平均（建议线性光颜色、线性 alpha 的累加器），再把该块的 RGBA **全部替换**为结果。不能只把 alpha 设为 0 来“遮盖”原文；PNG 规范也特别警告仅改 alpha 的 redaction 可能保留可恢复的原始颜色。[S15] 不为每个块分配整图副本，块内使用有界累加器。
- **文字：**用显式字体集和物理像素 `Metrics` 完成 shaping/layout，取 `SwashCache` 的 Mask/Color 结果；Mask coverage 乘标注 alpha，Color glyph 按 straight RGBA 处理，再由 Pixmap 统一预乘和 SourceOver。文本换行、回退和彩色 emoji 的支持范围应以测试矩阵为准，不由 `tiny-skia` 假定提供。

### 3.3 验证与验收建议（后续实施时执行）

- 用透明、半透明、重叠高亮验证 SourceOver；检查输出 PNG 是 RGBA8、非预乘 alpha，且解码后像素与 golden 一致。
- 用 1x/2x、奇数逻辑尺寸、边界线宽、箭头、Mosaic 边缘块验证“只缩放一次”；检查 Mosaic 后原 RGB 不残留。
- 用多行、CJK、RTL、缺字回退、彩色 emoji（若宣称支持）和半透明文字验证字体/alpha 适配；对比 GPUI 预览与最终图，而不是只比较布局尺寸。
- 在 macOS 实机读取 `NSPasteboardTypePNG`，检查 `setData_forType` 返回值，并粘贴到 Preview、常见目标应用；对另存文件与剪贴板 bytes 做 hash/解码一致性检查。
- 用接近预算上限的长图测量真实峰值内存、编码时间和失败路径；当前研究没有这些数据，不能预先承诺“任意超长”或某个固定分辨率。

## 4. 已验证与推断/未验证清单

| 项目 | 状态 |
|---|---|
| `tiny-skia` 的预乘 Pixmap、Stroke/SourceOver/抗锯齿、尺寸检查及 `encode_png` 整图复制路径 | **已由 0.12.0 文档/源码验证** [S1][S2][S3][S4][S5] |
| `cosmic-text` 的 shaping/fallback/rasterization API、物理像素 Metrics、Mask coverage 与 base alpha 的关系 | **已由 0.19.0 文档/源码验证** [S7][S8][S9] |
| PNG 的 RGBA8/非预乘 alpha、编码器 API、流式 writer 和 W3C 尺寸/合成规则 | **已由 crate 文档/源码和规范验证** [S11][S12][S13][S14][S15] |
| `NSPasteboard` 的 clear/setData/PNG 类型及 objc2 对应绑定 | **已由 Apple 和 crate 文档验证** [S16][S17][S18][S19][S20] |
| “原始图 → Pixmap → 标注 → 反预乘 → 一次编码 → 文件/剪贴板”作为 ArcShot 管线 | **架构建议/推断**，不是现成的单 crate API |
| 消费 `take_demultiplied` 比 `Pixmap::encode_png` 少一次整图复制 | **由源码直接推导；未做时间/峰值内存实测** |
| GPUI 与最终图逐像素一致、所有 macOS 应用都原样接受 PNG、任意字体/emoji 和任意超长截图 | **未验证，不能承诺** |

## 5. 来源列表

以下来源均为本次实际查阅的第一方 crate 文档/源码、项目 README、Apple 官方文档或 W3C 规范；版本号写在链接或条目中，避免把 `latest` 的未来变化误当成本次结论。

- **[S1] tiny-skia 官方 README**：CPU-only、填充/描边/混合、缺少文字与 ICC。<https://github.com/linebender/tiny-skia>
- **[S2] tiny-skia 0.12.0 `Pixmap` 文档**：预乘 RGBA、分配、宽度限制、编码 API。<https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Pixmap.html>
- **[S3] tiny-skia 0.12.0 `pixmap.rs` 源码**：预乘/反预乘、`from_vec`、`take_demultiplied`、`encode_png` 和 checked size 计算。<https://docs.rs/tiny-skia/0.12.0/src/tiny_skia/pixmap.rs.html>
- **[S4] tiny-skia 0.12.0 `Paint` 文档**：SourceOver、抗锯齿、colorspace。<https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Paint.html>
- **[S5] tiny-skia 0.12.0 `Stroke` 文档**：宽度、cap、join、dash。<https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Stroke.html>
- **[S6] tiny-skia 0.12.0 `ColorSpace` 文档**：Linear/Gamma2/SimpleSRGB/FullSRGBGamma。<https://docs.rs/tiny-skia/0.12.0/tiny_skia/enum.ColorSpace.html>
- **[S7] cosmic-text 0.19.0 crate 文档**：shaping、font discovery/fallback、layout、swash rasterization。<https://docs.rs/cosmic-text/0.19.0/cosmic_text/>
- **[S8] cosmic-text 0.19.0 API 文档**：`Buffer`、`SwashCache`、`FontSystem`、`Metrics`。<https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.Buffer.html>、<https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.SwashCache.html>、<https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.FontSystem.html>、<https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.Metrics.html>
- **[S9] cosmic-text 0.19.0 `swash.rs` 源码**：Mask/Color/SubpixelMask 及 coverage/alpha 回调实现。<https://docs.rs/cosmic-text/0.19.0/src/cosmic_text/swash.rs.html>
- **[S10] cosmic-text 0.19.0 `Cargo.toml`**：默认 feature、MSRV 和依赖。<https://docs.rs/crate/cosmic-text/0.19.0/source/Cargo.toml>
- **[S11] image-rs/image-png 官方 README**：纯 Rust PNG decoder/encoder。<https://github.com/image-rs/image-png>
- **[S12] png 0.18.0 `Encoder` 文档**：RGBA、8-bit、色彩和 header 配置。<https://docs.rs/png/0.18.0/png/struct.Encoder.html>
- **[S13] png 0.18.0 `Writer`/`StreamWriter` 文档**：写入、finish、流式输出。<https://docs.rs/png/0.18.0/png/struct.Writer.html>、<https://docs.rs/png/0.18.0/png/struct.StreamWriter.html>
- **[S14] W3C PNG Chunk Specifications**：IHDR 尺寸、color type 6、alpha 及非预乘规则。<https://www.w3.org/TR/PNG-Chunks.html>
- **[S15] W3C PNG Third Edition**：alpha、非预乘、linear intensity 合成及 alpha-only redaction 风险。<https://www.w3.org/TR/png-3/>
- **[S16] Apple `NSPasteboard`**：通用 pasteboard 与写入模型。<https://developer.apple.com/documentation/appkit/nspasteboard>
- **[S17] Apple `NSPasteboard` 写入 API**：`general`、`clearContents`、`setData(_:forType:)`、PNG 类型。<https://developer.apple.com/documentation/appkit/nspasteboard/general>、<https://developer.apple.com/documentation/appkit/nspasteboard/clearcontents()>、<https://developer.apple.com/documentation/appkit/nspasteboard/setdata(_:fortype:)>、<https://developer.apple.com/documentation/appkit/nspasteboard/pasteboardtype/png>
- **[S18] objc2-app-kit 0.3.2 `NSPasteboard` 文档**：Rust 绑定、返回值和线程 auto traits。<https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/struct.NSPasteboard.html>
- **[S19] objc2-foundation `NSData` 文档**：`with_bytes`、`from_vec`。<https://docs.rs/objc2-foundation/0.3.2/aarch64-apple-darwin/objc2_foundation/struct.NSData.html>
- **[S20] objc2-app-kit 0.3.2 `NSPasteboardTypePNG`**：PNG 静态类型绑定。<https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/static.NSPasteboardTypePNG.html>
- **[S21] Apple backing scale 文档**：`NSScreen`/`NSWindow.backingScaleFactor`。<https://developer.apple.com/documentation/appkit/nsscreen/backingscalefactor>、<https://developer.apple.com/documentation/appkit/nswindow/backingscalefactor>
- **[S22] Apple `NSView.convertToBacking`**：逻辑坐标到 backing 像素坐标。<https://developer.apple.com/documentation/appkit/nsview/converttobacking(_:)-2xx45>、<https://developer.apple.com/documentation/appkit/nsview/converttobacking(_:)-4ra9y>
- **[S23] GPUI 单位文档**：`DevicePixels`、`ScaledPixels`。<https://docs.rs/gpui/latest/gpui/struct.DevicePixels.html>、<https://docs.rs/gpui/latest/gpui/struct.ScaledPixels.html>
- **[S24] imageproc 0.27.0 `Canvas` 源码**：默认覆盖与 `Blend` 混合。<https://docs.rs/imageproc/0.27.0/src/imageproc/drawing/canvas.rs.html>
- **[S25] imageproc 0.27.0 `text.rs` 源码/API**：逐字符布局、coverage 混合、不支持换行。<https://docs.rs/imageproc/0.27.0/src/imageproc/drawing/text.rs.html>
- **[S26] imageproc 0.27.0 `line.rs` 源码**：Bresenham 线段实现。<https://docs.rs/imageproc/0.27.0/src/imageproc/drawing/line.rs.html>
- **[S27] imageproc 0.27.0 crate 文档**：基于 `image`、线性颜色空间假设。<https://docs.rs/imageproc/0.27.0/imageproc/>
- **[S28] ab_glyph 0.2.32 文档**：字体加载、glyph 缩放/定位/coverage 栅格化。<https://docs.rs/ab_glyph/0.2.32/ab_glyph/>、<https://docs.rs/ab_glyph/0.2.32/ab_glyph/struct.OutlinedGlyph.html>
- **[S29] image 0.25.10 文档/`Cargo.toml`**：buffer、`Limits` 和默认 features。<https://docs.rs/image/0.25.10/image/>、<https://docs.rs/image/0.25.10/image/struct.Limits.html>、<https://docs.rs/crate/image/0.25.10/source/Cargo.toml>
- **[S30] imageproc 0.27.0 `Cargo.toml`**：默认 features 与 `ab_glyph`/`image` 依赖。<https://docs.rs/crate/imageproc/0.27.0/source/Cargo.toml>

[S1]: https://github.com/linebender/tiny-skia
[S2]: https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Pixmap.html
[S3]: https://docs.rs/tiny-skia/0.12.0/src/tiny_skia/pixmap.rs.html
[S4]: https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Paint.html
[S5]: https://docs.rs/tiny-skia/0.12.0/tiny_skia/struct.Stroke.html
[S6]: https://docs.rs/tiny-skia/0.12.0/tiny_skia/enum.ColorSpace.html
[S7]: https://docs.rs/cosmic-text/0.19.0/cosmic_text/
[S8]: https://docs.rs/cosmic-text/0.19.0/cosmic_text/struct.Buffer.html
[S9]: https://docs.rs/cosmic-text/0.19.0/src/cosmic_text/swash.rs.html
[S10]: https://docs.rs/crate/cosmic-text/0.19.0/source/Cargo.toml
[S11]: https://github.com/image-rs/image-png
[S12]: https://docs.rs/png/0.18.0/png/struct.Encoder.html
[S13]: https://docs.rs/png/0.18.0/png/struct.Writer.html
[S14]: https://www.w3.org/TR/PNG-Chunks.html
[S15]: https://www.w3.org/TR/png-3/
[S16]: https://developer.apple.com/documentation/appkit/nspasteboard
[S17]: https://developer.apple.com/documentation/appkit/nspasteboard/setdata(_:fortype:)
[S18]: https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/struct.NSPasteboard.html
[S19]: https://docs.rs/objc2-foundation/0.3.2/aarch64-apple-darwin/objc2_foundation/struct.NSData.html
[S20]: https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/static.NSPasteboardTypePNG.html
[S21]: https://developer.apple.com/documentation/appkit/nsscreen/backingscalefactor
[S22]: https://developer.apple.com/documentation/appkit/nsview/converttobacking(_:)-2xx45
[S23]: https://docs.rs/gpui/latest/gpui/struct.DevicePixels.html
[S24]: https://docs.rs/imageproc/0.27.0/src/imageproc/drawing/canvas.rs.html
[S25]: https://docs.rs/imageproc/0.27.0/src/imageproc/drawing/text.rs.html
[S26]: https://docs.rs/imageproc/0.27.0/src/imageproc/drawing/line.rs.html
[S27]: https://docs.rs/imageproc/0.27.0/imageproc/
[S28]: https://docs.rs/ab_glyph/0.2.32/ab_glyph/
[S29]: https://docs.rs/image/0.25.10/image/struct.Limits.html
[S30]: https://docs.rs/crate/imageproc/0.27.0/source/Cargo.toml

## 6. 最终建议与未决风险

### 最终建议

采用“**GPUI 预览 / Rust 最终合成分离**”的 `tiny-skia + cosmic-text + png + objc2` 方案：物理像素和颜色/alpha 契约先行；Pixmap 内统一预乘和 SourceOver；Mosaic 做真正的块像素替换；文字 coverage 与标注 alpha 在适配层合并；消费 Pixmap 反预乘后只编码一次 PNG，文件和 `NSPasteboardTypePNG` 共享字节。先实现全帧、可配置预算的版本，超预算明确失败，再以实测结果决定是否需要分块合成；不要以 GPUI 截屏、`NSImage` 中转或 `StreamWriter` 单独充当这些问题的解法。

### 未决风险

1. **颜色管理与 ICC：**`tiny-skia` 不支持 ICC，且本研究没有完成 Display-P3/HDR 转换方案；输入必须先归一化为声明的 sRGB，或由后续专门决策处理 profile，否则颜色一致性仍有风险。[S1][S6]
2. **文字覆盖与预览一致性：**字体回退、CJK/RTL、彩色 emoji、SubpixelMask、hinting 和 GPUI 字体光栅化差异需要实机/golden 测试；特别是 `cosmic-text` Mask 的 base-alpha 适配不能省略。[S8][S9]
3. **长图资源上限：**全帧 Pixmap、原始图保留副本、PNG/NSData 和字体缓存的峰值尚未测量，预算数值和分块策略未定；当前建议不承诺任意长度。[S2][S3][S13]
4. **系统输出兼容性：**`setData_forType` 失败、pasteboard 通信异常、目标应用对 PNG 表示的转换，以及 adapter 的线程/生命周期，必须在 macOS 集成测试中验证；本次只验证了 Apple/绑定 API，未做真实粘贴。[S16][S17][S18]
5. **依赖与实现审计：**实施时仍需锁定 crate 版本、关闭不需要的默认 features、检查 Cargo.lock/MSRV，并验证 Mosaic 色彩算法和所有边界裁剪；这些是落地风险，不应被“纯 Rust”标签掩盖。[S10][S29][S30]
