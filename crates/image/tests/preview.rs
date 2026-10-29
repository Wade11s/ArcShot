//! IMAGE-LIMIT: a bounded UI preview never mutates the immutable captured base.
use arcshot_core::{CapturedFrame, ScreenGeometry};
#[test]
fn image_limit_01_preview_preserves_channels_and_padding_without_touching_base() {
    let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap();
    let bytes = vec![
        3, 2, 1, 255, 6, 5, 4, 255, 0, 0, 0, 0, 9, 8, 7, 255, 12, 11, 10, 255, 0, 0, 0, 0,
    ];
    let frame = CapturedFrame::try_from_srgb_bgra(g, 2, 2, 12, bytes.clone()).unwrap();
    let preview = arcshot_image::capture_preview(&frame).unwrap();
    assert_eq!([preview.width, preview.height], [2, 2]);
    assert_eq!(
        preview.bgra,
        vec![3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
    );
    assert_eq!(frame.bgra(), bytes);
}
#[test]
fn image_limit_02_full_screen_preview_is_below_four_million_pixels() {
    let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1470.0, 956.0, 2.0).unwrap();
    let frame =
        CapturedFrame::try_from_srgb_bgra(g, 2940, 1912, 11760, vec![255; 22_485_120]).unwrap();
    let preview = arcshot_image::capture_preview(&frame).unwrap();
    assert_eq!([preview.width, preview.height], [1470, 956]);
    assert_eq!(frame.geometry().pixel_size(), [2940, 1912]);
}
