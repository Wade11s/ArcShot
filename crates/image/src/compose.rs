//! Shared physical-pixel compositing and PNG encoding for approved output.
//!
//! Region capture and scroll output use this single crop/composite/encode path.
//! The input frame is immutable and never modified; no intermediate image is
//! written to disk.
use arcshot_core::{
    Annotation, AnnotationStyle, CapturedFrame, LineStyle, MAX_EDGE, MAX_PIXELS, MAX_PNG_BYTES,
    PixelRect, RectangleStyle, Selection,
};
use std::{
    error::Error,
    fmt,
    io::{self, Write},
};

/// A packed, opaque SDR sRGB RGBA surface produced from the immutable base.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ComposedImage {
    /// Admits an exact RGBA surface after checking dimensions and byte length.
    ///
    /// # Errors
    /// Rejects zero or oversized dimensions, budget overflow and length mismatch.
    pub fn try_new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ComposeError> {
        let length = checked_surface(width, height)?;
        if rgba.len() != length {
            return Err(ComposeError::Length);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
    /// Physical width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Physical height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Packed opaque RGBA bytes in sRGB.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Crops and copies the selected physical rectangle, converting BGRA to RGBA.
///
/// Row padding is skipped; only pixels inside the crop are read. Allocation is
/// reserved and checked before copying, and the crop must already satisfy the
/// document pixel and edge limits.
///
/// # Errors
/// Returns [`ComposeError::Crop`] for an out-of-frame crop and
/// [`ComposeError::ResourceLimit`] if the surface cannot be reserved.
pub fn compose_crop(frame: &CapturedFrame, crop: PixelRect) -> Result<ComposedImage, ComposeError> {
    let [frame_width, frame_height] = frame.geometry().pixel_size();
    if crop.width() == 0
        || crop.height() == 0
        || crop.x() + crop.width() > frame_width
        || crop.y() + crop.height() > frame_height
    {
        return Err(ComposeError::Crop);
    }
    let length = checked_surface(crop.width(), crop.height())?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length)
        .map_err(|_| ComposeError::ResourceLimit)?;
    let stride = frame.stride();
    for row in crop.y()..crop.y() + crop.height() {
        let start = row as usize * stride + crop.x() as usize * 4;
        let source = &frame.bgra()[start..start + crop.width() as usize * 4];
        for pixel in source.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(ComposedImage {
        width: crop.width(),
        height: crop.height(),
        rgba,
    })
}

/// Crops the selected physical rectangle and rasterizes every visible annotation.
///
/// Preview and final output call this one function, so the same document version
/// produces the same rectangle pixels. Layers draw bottom-to-top and every
/// annotation is clipped to the crop; retained geometry outside it is ignored,
/// never resampled.
///
/// # Errors
/// Returns [`ComposeError::Crop`] for an out-of-frame crop and the shared
/// budget/encoding errors of [`compose_crop`].
pub fn compose_document(
    frame: &CapturedFrame,
    crop: PixelRect,
    annotations: &[Annotation],
) -> Result<ComposedImage, ComposeError> {
    let mut image = compose_crop(frame, crop)?;
    let scale = frame.geometry().scale();
    for annotation in annotations {
        draw_annotation(&mut image, crop, scale, *annotation);
    }
    Ok(image)
}

/// Converts composed RGBA pixels to the BGRA layout GPUI's `RenderImage` expects.
///
/// # Errors
/// Returns [`ComposeError::ResourceLimit`] when the preview copy cannot be reserved.
pub fn preview_bgra(image: &ComposedImage) -> Result<Vec<u8>, ComposeError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(image.rgba.len())
        .map_err(|_| ComposeError::ResourceLimit)?;
    for pixel in image.rgba.as_chunks::<4>().0 {
        bytes.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(bytes)
}

fn physical(value: f64, scale: f64, origin: u32) -> i64 {
    let limit = f64::from(i32::MAX);
    ((value * scale).round().clamp(-limit, limit) as i64) - i64::from(origin)
}

fn span(start: i64, end: i64, limit: u32) -> std::ops::Range<usize> {
    let limit = i64::from(limit);
    (start.clamp(0, limit) as usize)..(end.clamp(0, limit) as usize)
}

/// Rasterizes one retained annotation in its creation order.
fn draw_annotation(image: &mut ComposedImage, crop: PixelRect, scale: f64, annotation: Annotation) {
    match annotation.style() {
        AnnotationStyle::Rectangle(style) => {
            if let Some(bounds) = annotation.shape().rectangle_bounds() {
                draw_rectangle(image, crop, scale, bounds, style);
            }
        }
        AnnotationStyle::Line(style) => {
            if let Some(segment) = annotation.shape().segment() {
                draw_line(image, crop, scale, segment, style);
            }
        }
        AnnotationStyle::Arrow(_) => draw_arrow(image, crop, scale, annotation),
    }
}

/// Draws one rectangle with an opaque stroke and an optional 20% same-colour fill.
fn draw_rectangle(
    image: &mut ComposedImage,
    crop: PixelRect,
    scale: f64,
    bounds: Selection,
    style: RectangleStyle,
) {
    let x0 = physical(bounds.x(), scale, crop.x());
    let y0 = physical(bounds.y(), scale, crop.y());
    let x1 = physical(bounds.x() + bounds.width(), scale, crop.x());
    let y1 = physical(bounds.y() + bounds.height(), scale, crop.y());
    let thickness = ((style.width().points() * scale).round() as i64).max(1);
    let [red, green, blue] = style.stroke().rgb();

    if style.fill() {
        for y in span(y0, y1, image.height) {
            for x in span(x0, x1, image.width) {
                blend_pixel(image, x, y, [red, green, blue]);
            }
        }
    }

    // The stroke is centred on the path: `thickness` pixels around each edge,
    // with the extra pixel (if any) on the right/bottom side.
    let half = thickness / 2;
    let left = x0 - half;
    let right = x1 - half;
    let top = y0 - half;
    let bottom = y1 - half;
    for y in top..bottom + thickness {
        if y < 0 || y >= i64::from(image.height) {
            continue;
        }
        for x in left..right + thickness {
            if x < 0 || x >= i64::from(image.width) {
                continue;
            }
            let on_vertical =
                (x >= left && x < left + thickness) || (x >= right && x < right + thickness);
            let on_horizontal =
                (y >= top && y < top + thickness) || (y >= bottom && y < bottom + thickness);
            if on_vertical || on_horizontal {
                set_pixel(image, x as usize, y as usize, [red, green, blue]);
            }
        }
    }
}

/// Logical-to-physical conversion for paths, centre-aligned like the rectangle stroke.
fn physical_f(value: f64, scale: f64, origin: u32) -> f64 {
    let limit = f64::from(i32::MAX);
    (value * scale).clamp(-limit, limit) - f64::from(origin)
}

/// Draws a butt-capped thick line, solid or split into width-scaled dashes.
fn draw_line(
    image: &mut ComposedImage,
    crop: PixelRect,
    scale: f64,
    segment: arcshot_core::Segment,
    style: LineStyle,
) {
    let start = [
        physical_f(segment.start()[0], scale, crop.x()),
        physical_f(segment.start()[1], scale, crop.y()),
    ];
    let end = [
        physical_f(segment.end()[0], scale, crop.x()),
        physical_f(segment.end()[1], scale, crop.y()),
    ];
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length = dx.hypot(dy);
    if length == 0.0 || !length.is_finite() {
        return;
    }
    let direction = [dx / length, dy / length];
    let perpendicular = [-direction[1], direction[0]];
    let half = ((style.width().points() * scale).round().max(1.0)) / 2.0;
    let [red, green, blue] = style.stroke().rgb();
    let color = [red, green, blue];
    let quad = |from: f64, to: f64| {
        let along = |distance: f64| {
            [
                start[0] + direction[0] * distance,
                start[1] + direction[1] * distance,
            ]
        };
        let (a, b) = (along(from), along(to));
        [
            [
                a[0] + perpendicular[0] * half,
                a[1] + perpendicular[1] * half,
            ],
            [
                b[0] + perpendicular[0] * half,
                b[1] + perpendicular[1] * half,
            ],
            [
                b[0] - perpendicular[0] * half,
                b[1] - perpendicular[1] * half,
            ],
            [
                a[0] - perpendicular[0] * half,
                a[1] - perpendicular[1] * half,
            ],
        ]
    };
    if !style.dash() {
        fill_polygon(image, &quad(0.0, length), color);
        return;
    }
    let (dash, gap) = style.dash_pattern();
    let dash = (dash * scale).round().max(1.0);
    let gap = (gap * scale).round().max(1.0);
    let period = dash + gap;
    // Only dashes intersecting the output surface are visited, so the loop is
    // bounded by the image diagonal even when the retained segment is huge.
    let limit = i64::from(i32::MAX) as f64;
    let margin = [-half.clamp(-limit, limit), -half.clamp(-limit, limit)];
    let Some((from, to)) = clip_parameter(
        start,
        end,
        [margin[0].max(-limit), margin[1].max(-limit)],
        [
            f64::from(image.width) + half,
            f64::from(image.height) + half,
        ],
    ) else {
        return;
    };
    let first = (from * length / period).floor().max(0.0) as u64;
    let last = (to * length / period).ceil().max(0.0) as u64;
    for index in first..=last {
        let begin = index as f64 * period;
        let finish = (begin + dash).min(length);
        if finish <= begin || finish <= from * length || begin >= to * length {
            continue;
        }
        fill_polygon(image, &quad(begin.max(0.0), finish), color);
    }
}

/// Clips a segment parameter range to an axis-aligned physical box.
fn clip_parameter(
    start: [f64; 2],
    end: [f64; 2],
    minimum: [f64; 2],
    maximum: [f64; 2],
) -> Option<(f64, f64)> {
    let direction = [end[0] - start[0], end[1] - start[1]];
    let (mut from, mut to) = (0.0_f64, 1.0_f64);
    for axis in 0..2 {
        if direction[axis] == 0.0 {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return None;
            }
            continue;
        }
        let mut enter = (minimum[axis] - start[axis]) / direction[axis];
        let mut leave = (maximum[axis] - start[axis]) / direction[axis];
        if enter > leave {
            std::mem::swap(&mut enter, &mut leave);
        }
        from = from.max(enter);
        to = to.min(leave);
        if from > to {
            return None;
        }
    }
    Some((from, to))
}

/// Fills the arrow silhouette from the shared logical outline.
fn draw_arrow(image: &mut ComposedImage, crop: PixelRect, scale: f64, annotation: Annotation) {
    let Some(outlines) = annotation.arrow_outlines() else {
        return;
    };
    let [red, green, blue] = annotation.style().stroke().rgb();
    for outline in outlines {
        let vertices: Vec<[f64; 2]> = outline
            .into_iter()
            .map(|[x, y]| {
                [
                    physical_f(x, scale, crop.x()),
                    physical_f(y, scale, crop.y()),
                ]
            })
            .collect();
        fill_polygon(image, &vertices, [red, green, blue]);
    }
}

/// Scanline-fills a polygon using pixel centres, clipped to the composed surface.
fn fill_polygon(image: &mut ComposedImage, vertices: &[[f64; 2]], color: [u8; 3]) {
    if vertices.len() < 3 {
        return;
    }
    let mut minimum_y = f64::INFINITY;
    let mut maximum_y = f64::NEG_INFINITY;
    for vertex in vertices {
        minimum_y = minimum_y.min(vertex[1]);
        maximum_y = maximum_y.max(vertex[1]);
    }
    if !minimum_y.is_finite() || !maximum_y.is_finite() {
        return;
    }
    let last_row = i64::from(image.height) - 1;
    let first_row = ((minimum_y - 0.5).ceil() as i64).max(0);
    let last_row = (((maximum_y - 0.5).ceil() as i64) - 1).min(last_row);
    if first_row > last_row {
        return;
    }
    let mut crossings: Vec<f64> = Vec::with_capacity(vertices.len());
    for row in first_row..=last_row {
        let center = row as f64 + 0.5;
        crossings.clear();
        let mut previous = vertices[vertices.len() - 1];
        for &current in vertices {
            let (x0, y0) = (previous[0], previous[1]);
            let (x1, y1) = (current[0], current[1]);
            if (y0 <= center) != (y1 <= center) {
                crossings.push(x0 + (center - y0) * (x1 - x0) / (y1 - y0));
            }
            previous = current;
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.as_chunks::<2>().0 {
            let left = ((pair[0] - 0.5).ceil() as i64).max(0);
            let right = (((pair[1] - 0.5).ceil() as i64) - 1).min(i64::from(image.width) - 1);
            if left > right {
                continue;
            }
            for x in left..=right {
                set_pixel(image, x as usize, row as usize, color);
            }
        }
    }
}

fn set_pixel(image: &mut ComposedImage, x: usize, y: usize, color: [u8; 3]) {
    let index = (y * image.width as usize + x) * 4;
    image.rgba[index..index + 3].copy_from_slice(&color);
    image.rgba[index + 3] = 255;
}

/// Composites the fill at 20% opacity (51/255) over the base pixel.
fn blend_pixel(image: &mut ComposedImage, x: usize, y: usize, color: [u8; 3]) {
    let index = (y * image.width as usize + x) * 4;
    for (channel, source) in color.into_iter().enumerate() {
        let base = u32::from(image.rgba[index + channel]);
        image.rgba[index + channel] = ((u32::from(source) * 51 + base * 204 + 127) / 255) as u8;
    }
}

/// Encodes a composed surface as an 8-bit RGBA PNG with an sRGB chunk.
///
/// The immutable frame and composed pixels are the only inputs; the returned
/// bytes are reused for both clipboard and explicit file output.
///
/// # Errors
/// Returns [`ComposeError::Encode`] if PNG encoding fails and
/// [`ComposeError::ResourceLimit`] when the PNG exceeds V1's byte budget.
pub fn encode_png(image: &ComposedImage) -> Result<Vec<u8>, ComposeError> {
    let mut budget = BudgetWriter::new(MAX_PNG_BYTES);
    {
        let mut encoder = png::Encoder::new(&mut budget, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header().map_err(|_| ComposeError::Encode)?;
        writer
            .write_image_data(&image.rgba)
            .map_err(|_| ComposeError::Encode)?;
        writer.finish().map_err(|_| ComposeError::Encode)?;
    }
    if budget.exceeded {
        return Err(ComposeError::ResourceLimit);
    }
    Ok(budget.bytes)
}

/// A writer that refuses a write before the encoded PNG can exceed its budget.
struct BudgetWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl BudgetWriter {
    const fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            exceeded: false,
        }
    }
}
impl Write for BudgetWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("PNG output budget exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn checked_surface(width: u32, height: u32) -> Result<usize, ComposeError> {
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return Err(ComposeError::Dimensions);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ComposeError::Dimensions)?;
    if pixels > MAX_PIXELS {
        return Err(ComposeError::ResourceLimit);
    }
    pixels.checked_mul(4).ok_or(ComposeError::Dimensions)
}

/// Composition or encoding failed before any output side effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposeError {
    /// The crop leaves the captured frame.
    Crop,
    /// Image dimensions are empty or exceed V1's edge limit.
    Dimensions,
    /// The surface or encoded bytes exceed V1's budget.
    ResourceLimit,
    /// The supplied RGBA byte length does not match its dimensions.
    Length,
    /// PNG encoding failed.
    Encode,
}
impl fmt::Display for ComposeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Crop => "crop is outside the captured frame",
            Self::Dimensions => "image dimensions are invalid",
            Self::ResourceLimit => "image exceeds the output budget",
            Self::Length => "RGBA byte length does not match dimensions",
            Self::Encode => "PNG encoding failed",
        })
    }
}
impl Error for ComposeError {}
