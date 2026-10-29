//! Image assets and future image processing for ArcShot.

use arcshot_core::StatusItemIcon;

/// Returns the built-in status item icon as an encoded PNG.
#[must_use]
pub const fn status_item_icon() -> StatusItemIcon {
    StatusItemIcon::from_png(include_bytes!("../assets/status-item.png"))
}
