//! IMAGE-LIMIT: frozen presentation preserves physical-pixel detail and the captured base.
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
fn image_limit_02_frozen_preview_preserves_single_pixel_lines_at_native_resolution() {
    let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1470.0, 956.0, 2.0).unwrap();
    let mut bytes = [0, 0, 0, 255].repeat(2940 * 1912);
    // One-pixel lines on odd coordinates disappear with the former 2x subsampling.
    for y in 0..1912 {
        let offset = (y * 2940 + 1) * 4;
        bytes[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    for x in 0..2940 {
        let offset = (2940 + x) * 4;
        bytes[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    bytes[22_485_116..].copy_from_slice(&[3, 2, 1, 255]);
    let frame = CapturedFrame::try_from_srgb_bgra(g, 2940, 1912, 11760, bytes).unwrap();
    let preview = arcshot_image::capture_preview(&frame).unwrap();
    let pixel = |x: usize, y: usize| {
        let offset = (y * preview.width as usize + x) * 4;
        &preview.bgra[offset..offset + 4]
    };
    assert_eq!(pixel(0, 0), [0, 0, 0, 255]);
    assert_eq!(
        pixel(1, 0),
        [255, 255, 255, 255],
        "vertical hairline was lost"
    );
    assert_eq!(
        pixel(0, 1),
        [255, 255, 255, 255],
        "horizontal hairline was lost"
    );
    assert_eq!([preview.width, preview.height], [2940, 1912]);
    assert_eq!(pixel(2, 2), [0, 0, 0, 255]);
    assert_eq!(pixel(2939, 1911), [3, 2, 1, 255], "last pixel was cropped");
    assert_eq!(frame.geometry().pixel_size(), [2940, 1912]);
}
