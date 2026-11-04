//! Image assets, compositing and encoding for ArcShot.

mod compose;
mod preview;
mod text;
pub use compose::{
    ComposeError, ComposedImage, compose_crop, compose_document, compose_document_with_font,
    encode_png, preview_bgra,
};
pub use preview::{CapturePreview, PreviewError, capture_preview};
pub use text::{BitmapFont, TextFont, TextGlyphs, TextMetrics, measure_text, tint};

use arcshot_core::StatusItemIcon;

/// Returns the built-in status item icon as an encoded PNG.
#[must_use]
pub const fn status_item_icon() -> StatusItemIcon {
    StatusItemIcon::from_png(include_bytes!("../assets/status-item.png"))
}
