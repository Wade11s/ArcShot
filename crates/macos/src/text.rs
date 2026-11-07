//! System sans-serif text measurement and rasterization through Core Text.
//!
//! The deterministic 5x7 golden font lives in `arcshot-image`; the product's
//! committed text must use the system face so CJK, emoji and fallback scripts
//! keep their real glyphs. This module owns the native half: Core Text lays out
//! one non-wrapping line per `\n`, measures the box in logical points and draws
//! it into a premultiplied RGBA surface at a physical scale.
//!
//! Colour glyphs (emoji) keep their own colours, so the surface carries real
//! RGBA rather than a coverage mask. Text pixels use the caller's style colour.
//! Everything runs inside an autorelease pool and is safe to call off the main
//! thread, which is where preview and output composition run.

use arcshot_core::{MAX_EDGE, MAX_PIXELS};
use core_foundation::{
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    boolean::CFBoolean,
    string::{CFString, CFStringRef},
};
use core_graphics::{
    base::kCGImageAlphaPremultipliedLast, color_space::CGColorSpace, context::CGContext,
    geometry::CGRect,
};
use core_text::{
    font::{self, CTFont, kCTFontSystemFontType},
    line::CTLine,
    string_attributes::{kCTFontAttributeName, kCTForegroundColorFromContextAttributeName},
};
use objc2::rc::autoreleasepool;

/// Logical text metrics used by the app to size and position the text box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    /// Number of lines after splitting on `\n`.
    pub line_count: usize,
    /// Widest line in logical points.
    pub max_line_width: f64,
    /// Logical line height (ascent + descent + leading).
    pub line_height: f64,
    /// Distance from the box top to the first baseline, in logical points.
    pub ascent: f64,
}

/// One rasterized text box in physical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRaster {
    /// Physical width in pixels.
    pub width: u32,
    /// Physical height in pixels.
    pub height: u32,
    /// Premultiplied RGBA, row-major, top-left origin.
    pub rgba: Vec<u8>,
}

/// One laid-out multi-line text box at a given point size.
struct Layout {
    lines: Vec<CTLine>,
    max_line_width: f64,
    ascent: f64,
    descent: f64,
    leading: f64,
}

impl Layout {
    /// Lays out one non-wrapping line per `\n` with the system UI font.
    ///
    /// Returns `None` for a non-finite or non-positive size. The maximum
    /// ascent and descent across lines keep tall glyphs (emoji) inside the box.
    fn new(content: &str, size_points: f64) -> Option<Self> {
        if !size_points.is_finite() || size_points <= 0.0 {
            return None;
        }
        autoreleasepool(|_| {
            let system = font::new_ui_font_for_language(kCTFontSystemFontType, size_points, None);
            let mut ascent = system.ascent();
            let mut descent = system.descent();
            let mut lines = Vec::new();
            let mut max_line_width: f64 = 0.0;
            for text in content.split('\n') {
                let line = attributed_line(&system, text);
                let bounds = line.get_typographic_bounds();
                ascent = ascent.max(bounds.ascent);
                descent = descent.max(bounds.descent);
                max_line_width = max_line_width.max(bounds.width);
                lines.push(line);
            }
            Some(Self {
                lines,
                max_line_width,
                ascent,
                descent,
                leading: system.leading(),
            })
        })
    }

    fn line_height(&self) -> f64 {
        (self.ascent + self.descent + self.leading).max(f64::MIN_POSITIVE)
    }
}

/// Builds one Core Text line with the system font as its only attribute.
fn attributed_line(font: &CTFont, text: &str) -> CTLine {
    let string = CFString::new(text);
    let mut attributed = CFMutableAttributedString::new();
    attributed.replace_str(&string, CFRange::init(0, 0));
    let range = CFRange::init(0, string.char_len());
    attributed.set_attribute(range, font_attribute(), font);
    // Without this attribute Core Text paints its default black and ignores the
    // caller's style colour. Colour glyphs still keep their own colours.
    attributed.set_attribute(range, foreground_from_context(), &CFBoolean::true_value());
    CTLine::new_with_attributed_string(attributed.as_concrete_TypeRef())
}

/// The Core Text font attribute name.
///
/// SAFETY: `kCTFontAttributeName` is a valid, immutable CFString constant
/// exported by Core Text for the lifetime of the process.
fn font_attribute() -> CFStringRef {
    unsafe { kCTFontAttributeName }
}

/// The "use the context fill colour" attribute name.
///
/// SAFETY: `kCTForegroundColorFromContextAttributeName` is a valid, immutable
/// CFString constant exported by Core Text for the lifetime of the process.
fn foreground_from_context() -> CFStringRef {
    unsafe { kCTForegroundColorFromContextAttributeName }
}

/// Measures multi-line text in logical points with the system sans-serif face.
///
/// The text never wraps: each `\n` starts a new line and the width is the
/// widest line's typographic width. Returns `None` for an unrenderable size.
#[must_use]
pub fn measure_text(content: &str, size_points: f64) -> Option<TextMeasure> {
    let layout = Layout::new(content, size_points)?;
    Some(TextMeasure {
        line_count: layout.lines.len(),
        max_line_width: layout.max_line_width,
        line_height: layout.line_height(),
        ascent: layout.ascent,
    })
}

/// Rasterizes multi-line text at a physical scale with the system sans-serif
/// face and the given style colour.
///
/// The bitmap box is `ceil(measure * scale)`. Every line is drawn at its
/// typographic baseline, with the glyphs clipped to the box. Returns `None`
/// for an unrenderable size or scale.
#[must_use]
pub fn rasterize_text(
    content: &str,
    size_points: f64,
    scale: f64,
    color: [u8; 3],
) -> Option<TextRaster> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let measure = measure_text(content, size_points)?;
    let (width, height) = bitmap_size(&measure, scale)?;
    let layout = Layout::new(content, size_points * scale)?;
    autoreleasepool(|_| {
        let width_usize = width as usize;
        let height_usize = height as usize;
        let space = CGColorSpace::create_device_rgb();
        let mut context = CGContext::create_bitmap_context(
            None,
            width_usize,
            height_usize,
            8,
            width_usize * 4,
            &space,
            kCGImageAlphaPremultipliedLast,
        );
        context.clear_rect(CGRect::new(
            &core_graphics::geometry::CGPoint::new(0.0, 0.0),
            &core_graphics::geometry::CGSize::new(f64::from(width), f64::from(height)),
        ));
        context.set_should_antialias(true);
        let [red, green, blue] = color;
        context.set_rgb_fill_color(
            f64::from(red) / 255.0,
            f64::from(green) / 255.0,
            f64::from(blue) / 255.0,
            1.0,
        );
        // The bitmap context is y-up with memory row 0 at the top, so the first
        // baseline sits `height - ascent` from the origin; each following line
        // moves down by one line height.
        let step = measure.line_height * scale;
        let bottom = f64::from(height);
        for (index, line) in layout.lines.iter().enumerate() {
            context.set_text_position(0.0, bottom - (layout.ascent + index as f64 * step));
            line.draw(&context);
        }
        context.flush();
        let mut rgba = context.data().to_vec();
        rgba.truncate(width_usize * height_usize * 4);
        Some(TextRaster {
            width,
            height,
            rgba,
        })
    })
}

/// Physical box for one measured text, clamped to V1's image budgets.
fn bitmap_size(measure: &TextMeasure, scale: f64) -> Option<(u32, u32)> {
    let width = (measure.max_line_width * scale).ceil().max(1.0);
    let height = (measure.line_count as f64 * measure.line_height * scale)
        .ceil()
        .max(1.0);
    if !width.is_finite() || !height.is_finite() {
        return None;
    }
    let width = width.min(f64::from(MAX_EDGE)) as u64;
    let height = height.min(f64::from(MAX_EDGE)) as u64;
    let pixels = width * height;
    let height = if pixels > MAX_PIXELS as u64 {
        (MAX_PIXELS as u64 / width).max(1)
    } else {
        height
    };
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}
