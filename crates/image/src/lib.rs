//! Image assets, compositing and encoding for ArcShot.

mod compose;
mod preview;
pub use compose::{ComposeError, ComposedImage, compose_crop, encode_png};
pub use preview::{CapturePreview, PreviewError, capture_preview};

use arcshot_core::StatusItemIcon;

/// Returns the built-in status item icon as an encoded PNG.
#[must_use]
pub const fn status_item_icon() -> StatusItemIcon {
    StatusItemIcon::from_png(include_bytes!("../assets/status-item.png"))
}
