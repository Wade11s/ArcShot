//! Presentation geometry for the post-copy completion thumbnail (I-11 §7).
//!
//! The thumbnail is proportional to the complete long image, placed at the
//! bottom-right of the built-in display and clamped on screen. It never feeds
//! back into the document crop or annotation coordinates.

/// Widest thumbnail window in logical points.
pub const MAX_WIDTH: f64 = 420.0;
/// Tallest thumbnail window in logical points.
pub const MAX_HEIGHT: f64 = 320.0;
/// Screen margin kept around the thumbnail in logical points.
pub const MARGIN: f64 = 16.0;

/// The resolved thumbnail window rectangle in display-local logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewLayout {
    /// Window left edge.
    pub x: f64,
    /// Window top edge.
    pub y: f64,
    /// Window width.
    pub width: f64,
    /// Window height.
    pub height: f64,
}

/// Fits one image proportionally into the thumbnail box at the bottom-right.
///
/// `image_logical` is the complete long image in logical points and `viewport`
/// is the built-in display in logical points. The result always keeps the
/// image aspect ratio; the view letterboxes inside the window when the box
/// itself cannot match it.
#[must_use]
pub fn preview_layout(image_logical: [f64; 2], viewport: [f64; 2]) -> PreviewLayout {
    let available_width = (viewport[0] - MARGIN * 2.0).max(0.0);
    let available_height = (viewport[1] - MARGIN * 2.0).max(0.0);
    let scale = (MAX_WIDTH / image_logical[0])
        .min(MAX_HEIGHT / image_logical[1])
        .min(available_width / image_logical[0])
        .min(available_height / image_logical[1]);
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let width = (image_logical[0] * scale).round().max(1.0);
    let height = (image_logical[1] * scale).round().max(1.0);
    let minimum = MARGIN;
    let maximum_x = (viewport[0] - width - MARGIN).max(minimum);
    let maximum_y = (viewport[1] - height - MARGIN).max(minimum);
    PreviewLayout {
        x: (viewport[0] - width - MARGIN).clamp(minimum, maximum_x),
        y: (viewport[1] - height - MARGIN).clamp(minimum, maximum_y),
        width,
        height,
    }
}
