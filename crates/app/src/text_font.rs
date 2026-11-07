//! The product's system sans-serif text font.
//!
//! `arcshot-image` owns the compositor and its golden 5x7 font; `arcshot-macos`
//! owns the Core Text adapter. The app is the only crate that depends on both,
//! so the bridge lives here. Preview, output and the live editor all measure and
//! rasterize through this one instance, which keeps the committed text box and
//! the drawn glyphs in step.

use arcshot_image::{TextFont, TextGlyphs, TextMetrics};

/// Zero-sized bridge to the macOS system UI face.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFont;

static SYSTEM_FONT: SystemFont = SystemFont;

/// The single font instance shared by preview, output and editor layout.
#[must_use]
pub fn system_font() -> &'static SystemFont {
    &SYSTEM_FONT
}

/// Measures multi-line text with the system face, falling back to the built-in
/// metrics only if Core Text rejects the size.
#[must_use]
pub fn measure(content: &str, size_points: f64) -> TextMetrics {
    arcshot_image::measure_text_with_font(system_font(), content, size_points)
        .unwrap_or_else(|| arcshot_image::measure_text(content, size_points))
}

impl TextFont for SystemFont {
    fn measure(&self, content: &str, size_points: f64) -> Option<TextMetrics> {
        arcshot_macos::measure_text(content, size_points).map(|measure| TextMetrics {
            line_count: measure.line_count,
            max_line_width: measure.max_line_width,
            line_height: measure.line_height,
        })
    }

    fn rasterize(
        &self,
        content: &str,
        size_points: f64,
        scale: f64,
        color: [u8; 3],
    ) -> Option<TextGlyphs> {
        let raster = arcshot_macos::rasterize_text(content, size_points, scale, color)?;
        TextGlyphs::new(raster.width, raster.height, raster.rgba)
    }
}
