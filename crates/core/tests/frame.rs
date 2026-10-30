//! APP-FRAME and CORE-COORD behavior at the neutral frame boundary.
use arcshot_core::{CapturedFrame, FrameError, ScreenGeometry};

#[test]
fn core_coord_01_screen_origin_is_not_a_display_local_source_origin() {
    let geometry = ScreenGeometry::try_new(1, -1470.0, 40.0, 1470.0, 956.0, 2.0).unwrap();
    assert_eq!(geometry.source_rect(), [0.0, 0.0, 1470.0, 956.0]);
    assert_eq!(geometry.pixel_size(), [2940, 1912]);
    assert_eq!(geometry.screen_origin(), [-1470.0, 40.0]);
}

#[test]
fn app_frame_01_accepts_opaque_named_srgb_bgra_with_row_padding() {
    let geometry = ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap();
    let bytes = vec![
        3, 2, 1, 255, 6, 5, 4, 255, 0, 0, 0, 0, 9, 8, 7, 255, 12, 11, 10, 255, 0, 0, 0, 0,
    ];
    let frame = CapturedFrame::try_from_srgb_bgra(geometry, 2, 2, 12, bytes).unwrap();
    assert_eq!(frame.stride(), 12);
    assert_eq!(&frame.bgra()[12..16], &[9, 8, 7, 255]);
}

#[test]
fn app_frame_02_rejects_malformed_frames_before_admission() {
    let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap();
    for (w, h, stride, bytes, error) in [
        (1, 2, 8, vec![255; 16], FrameError::Dimensions),
        (2, 2, 7, vec![255; 14], FrameError::Stride),
        (2, 2, usize::MAX, vec![], FrameError::Stride),
        (2, 2, 8, vec![255; 15], FrameError::ByteLength),
        (2, 2, 8, vec![0; 16], FrameError::Alpha),
        (2, 2, 1024 * 1024 * 1024, vec![], FrameError::Budget),
    ] {
        assert_eq!(
            CapturedFrame::try_from_srgb_bgra(g, w, h, stride, bytes).unwrap_err(),
            error
        );
    }
}

#[test]
fn core_coord_02_rejects_nonfinite_nonretina_and_oversized_geometry() {
    for (w, h, scale) in [
        (f64::NAN, 956.0, 2.0),
        (1470.0, 956.0, 1.0),
        (0.0, 1.0, 2.0),
        (8193.0, 1.0, 2.0),
        (5000.0, 5000.0, 2.0),
        (1.1, 1.0, 2.0),
        (f64::INFINITY, 1.0, 2.0),
    ] {
        assert!(
            ScreenGeometry::try_new(1, 0.0, 0.0, w, h, scale).is_err(),
            "{w} {h} {scale}"
        );
    }
}
