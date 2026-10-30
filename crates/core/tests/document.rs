//! CORE-COORD: one logical selection converts directly to physical output.
use arcshot_core::{
    CapturedFrame, Document, DocumentError, MAX_PIXELS, PixelRect, ScreenGeometry, Selection,
};

fn screen() -> ScreenGeometry {
    ScreenGeometry::try_new(1, 0.0, 0.0, 100.0, 50.0, 2.0).unwrap()
}

fn frame() -> CapturedFrame {
    let [w, h] = screen().pixel_size();
    CapturedFrame::try_from_srgb_bgra(
        screen(),
        w,
        h,
        w as usize * 4,
        vec![255; w as usize * h as usize * 4],
    )
    .unwrap()
}

#[test]
fn core_coord_03_drag_converts_once_from_logical_points_to_2x_pixels() {
    let selection = Selection::from_drag([10.0, 5.0], [40.0, 25.0]).unwrap();
    assert_eq!(
        (
            selection.x(),
            selection.y(),
            selection.width(),
            selection.height()
        ),
        (10.0, 5.0, 30.0, 20.0)
    );
    let rect = selection.pixel_rect(screen()).unwrap();
    assert_eq!(
        (rect.x(), rect.y(), rect.width(), rect.height()),
        (20, 10, 60, 40)
    );
}

#[test]
fn core_coord_04_reversed_drag_is_normalized_without_double_scaling() {
    let selection = Selection::from_drag([40.0, 25.0], [10.0, 5.0]).unwrap();
    let rect = selection.pixel_rect(screen()).unwrap();
    assert_eq!(
        (rect.x(), rect.y(), rect.width(), rect.height()),
        (20, 10, 60, 40)
    );
}

#[test]
fn core_coord_05_zero_or_malformed_drag_never_becomes_a_selection() {
    assert_eq!(
        Selection::from_drag([10.0, 10.0], [10.0, 25.0]),
        Err(DocumentError::Empty)
    );
    assert_eq!(
        Selection::from_drag([10.0, 10.0], [30.0, 10.0]),
        Err(DocumentError::Empty)
    );
    assert_eq!(
        Selection::from_drag([f64::NAN, 0.0], [10.0, 10.0]),
        Err(DocumentError::Geometry)
    );
    assert_eq!(
        Selection::try_new(0.0, 0.0, -1.0, 10.0),
        Err(DocumentError::Geometry)
    );
}

#[test]
fn core_coord_06_selection_outside_screen_or_without_pixels_is_rejected() {
    assert_eq!(
        Selection::try_new(90.0, 10.0, 20.0, 20.0)
            .unwrap()
            .pixel_rect(screen()),
        Err(DocumentError::OutOfBounds)
    );
    assert_eq!(
        Selection::try_new(-1.0, 0.0, 10.0, 10.0)
            .unwrap()
            .pixel_rect(screen()),
        Err(DocumentError::OutOfBounds)
    );
    // 0.2 points round to zero physical pixels at 2x.
    assert_eq!(
        Selection::try_new(10.0, 10.0, 0.2, 10.0)
            .unwrap()
            .pixel_rect(screen()),
        Err(DocumentError::Empty)
    );
}

#[test]
fn core_coord_07_full_screen_selection_keeps_original_physical_size() {
    let selection = Selection::try_new(0.0, 0.0, 100.0, 50.0).unwrap();
    let rect = selection.pixel_rect(screen()).unwrap();
    assert_eq!(
        (rect.x(), rect.y(), rect.width(), rect.height()),
        (0, 0, 200, 100)
    );
}

#[test]
fn image_limit_03_physical_rect_rejects_edge_and_pixel_budget_before_allocation() {
    assert_eq!(
        PixelRect::try_new(0, 0, 16_385, 1),
        Err(DocumentError::Dimensions)
    );
    assert_eq!(
        PixelRect::try_new(0, 0, 0, 10),
        Err(DocumentError::Dimensions)
    );
    assert!(PixelRect::try_new(0, 0, 16_384, 1).is_ok());
    // 16,384 x 1,466 exceeds 24,000,000 physical pixels.
    let over_budget = (MAX_PIXELS / 16_384 + 1) as u32;
    assert_eq!(
        PixelRect::try_new(0, 0, 16_384, over_budget),
        Err(DocumentError::Dimensions)
    );
}

#[test]
fn core_document_01_selection_changes_bump_version_and_invalid_input_keeps_the_old_one() {
    let mut document = Document::new(frame());
    assert_eq!(document.version(), 0);
    assert_eq!(document.crop(), Err(DocumentError::NoSelection));

    let selection = Selection::try_new(10.0, 10.0, 20.0, 10.0).unwrap();
    document.select(selection).unwrap();
    assert_eq!(document.version(), 1);
    assert_eq!(document.selection(), Some(selection));
    assert_eq!(
        document.crop().unwrap(),
        PixelRect::try_new(20, 20, 40, 20).unwrap()
    );

    // Selecting the same rectangle is not a change.
    document.select(selection).unwrap();
    assert_eq!(document.version(), 1);

    // An invalid rectangle does not replace the valid one.
    assert!(
        document
            .select(Selection::try_new(95.0, 40.0, 10.0, 10.0).unwrap())
            .is_err()
    );
    assert_eq!(document.selection(), Some(selection));
    assert_eq!(document.version(), 1);
}
