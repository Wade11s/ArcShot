//! APP-EFFECT: the completion thumbnail keeps the whole long image proportional,
//! sits at the bottom-right of the built-in display and stays on screen.
use arcshot_app::{MARGIN, MAX_HEIGHT, MAX_WIDTH, preview_layout};

/// The built-in display in logical points (I-13 acceptance geometry).
const VIEWPORT: [f64; 2] = [1470.0, 956.0];

#[test]
fn app_effect_50_a_tall_long_image_fits_the_height_cap_proportionally() {
    // 580x3700 logical points: the height cap dominates, the width follows.
    let layout = preview_layout([580.0, 3700.0], VIEWPORT);
    assert_eq!(layout.height, MAX_HEIGHT);
    assert_eq!(layout.width, 50.0);
    assert_eq!(layout.x, VIEWPORT[0] - layout.width - MARGIN);
    assert_eq!(layout.y, VIEWPORT[1] - layout.height - MARGIN);
    // The aspect ratio survives the integer rounding to within one point.
    let ratio = layout.width / layout.height;
    assert!((ratio - 580.0 / 3700.0).abs() < 0.01, "ratio {ratio}");
}

#[test]
fn app_effect_51_a_wide_image_fits_the_width_cap_and_stays_in_the_margin() {
    let layout = preview_layout([2940.0, 1912.0], VIEWPORT);
    assert_eq!(layout.width, MAX_WIDTH);
    assert_eq!(
        layout.height,
        (MAX_WIDTH * 1912.0 / 2940.0).round(),
        "height follows the width cap"
    );
    assert!(layout.x >= MARGIN && layout.y >= MARGIN);
    assert!(layout.x + layout.width <= VIEWPORT[0] - MARGIN);
    assert!(layout.y + layout.height <= VIEWPORT[1] - MARGIN);
}

#[test]
fn app_effect_52_a_small_viewport_clamps_the_box_without_losing_the_margin() {
    let layout = preview_layout([1000.0, 800.0], [500.0, 400.0]);
    assert!(layout.width <= 500.0 - MARGIN * 2.0);
    assert!(layout.height <= 400.0 - MARGIN * 2.0);
    assert!(layout.x >= MARGIN && layout.y >= MARGIN);
    assert!(layout.x + layout.width <= 500.0 - MARGIN);
    assert!(layout.y + layout.height <= 400.0 - MARGIN);
}
