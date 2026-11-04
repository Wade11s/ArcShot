//! CORE-EDIT: selection move/resize, reselect, annotation retention and history.
use arcshot_core::{CapturedFrame, Document, DragTarget, ResizeHandle, ScreenGeometry, Selection};

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

fn selection(x: f64, y: f64, width: f64, height: f64) -> Selection {
    Selection::try_new(x, y, width, height).unwrap()
}

/// A document with one selection and two layer-ordered annotations.
fn annotated() -> Document {
    let mut document = Document::new(frame());
    document.select(selection(10.0, 5.0, 30.0, 20.0)).unwrap();
    document
        .add_rectangle(selection(12.0, 7.0, 4.0, 4.0))
        .unwrap();
    document
        .add_rectangle(selection(20.0, 10.0, 8.0, 6.0))
        .unwrap();
    document
}

fn bounds(document: &Document) -> Vec<(f64, f64, f64, f64)> {
    document
        .annotations()
        .iter()
        .map(|annotation| {
            let bounds = annotation.shape().rectangle_bounds().unwrap();
            (bounds.x(), bounds.y(), bounds.width(), bounds.height())
        })
        .collect()
}

#[test]
fn core_edit_01_one_move_drag_is_one_history_step_that_carries_annotations() {
    let mut document = annotated();
    let version = document.version();

    assert_eq!(document.begin_drag([20.0, 10.0]), Some(DragTarget::Move));
    document.update_drag([24.0, 13.0]);
    // The live preview follows the drag without recording a step yet.
    assert_eq!(document.selection(), Some(selection(14.0, 8.0, 30.0, 20.0)));
    assert_eq!(
        bounds(&document),
        vec![(16.0, 10.0, 4.0, 4.0), (24.0, 13.0, 8.0, 6.0)]
    );
    assert_eq!(document.version(), version);
    // Undo cannot split a held drag; the shortcut waits for release.
    assert!(!document.undo().unwrap());
    assert_eq!(document.selection(), Some(selection(14.0, 8.0, 30.0, 20.0)));
    assert!(document.dragging());

    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert!(!document.dragging());

    // One undo restores the whole drag, including every annotation.
    assert!(document.undo().unwrap());
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 30.0, 20.0)));
    assert_eq!(
        bounds(&document),
        vec![(12.0, 7.0, 4.0, 4.0), (20.0, 10.0, 8.0, 6.0)]
    );
    assert!(document.redo().unwrap());
    assert_eq!(document.selection(), Some(selection(14.0, 8.0, 30.0, 20.0)));
    assert_eq!(
        bounds(&document),
        vec![(16.0, 10.0, 4.0, 4.0), (24.0, 13.0, 8.0, 6.0)]
    );
}

#[test]
fn core_edit_02_moving_the_selection_clamps_to_the_screen_and_moves_annotations_by_the_applied_delta()
 {
    let mut document = annotated();
    document.begin_drag([20.0, 10.0]);
    // The pointer leaves the screen; the selection stops at the bottom-right edge.
    document.update_drag([95.0, 60.0]);
    assert_eq!(
        document.selection(),
        Some(selection(70.0, 30.0, 30.0, 20.0))
    );
    assert_eq!(
        bounds(&document),
        vec![(72.0, 32.0, 4.0, 4.0), (80.0, 35.0, 8.0, 6.0)]
    );

    // Dragging past the top-left edge clamps back to the origin.
    document.update_drag([-40.0, -40.0]);
    assert_eq!(document.selection(), Some(selection(0.0, 0.0, 30.0, 20.0)));
    assert_eq!(
        bounds(&document),
        vec![(2.0, 2.0, 4.0, 4.0), (10.0, 5.0, 8.0, 6.0)]
    );
}

#[test]
fn core_edit_03_a_no_op_drag_records_no_history_step_or_version() {
    let mut document = annotated();
    let version = document.version();
    // A click inside the selection changes nothing.
    document.begin_drag([20.0, 10.0]);
    document.update_drag([20.0, 10.0]);
    assert!(!document.end_drag().unwrap());
    assert_eq!(document.version(), version);
    assert_eq!(document.annotations().len(), 2);

    // A full-screen selection has nowhere to move.
    let mut full = Document::new(frame());
    full.select(selection(0.0, 0.0, 100.0, 50.0)).unwrap();
    let full_version = full.version();
    full.begin_drag([50.0, 25.0]);
    full.update_drag([90.0, 45.0]);
    assert!(!full.end_drag().unwrap());
    assert_eq!(full.version(), full_version);

    // Undo skips both no-ops and reaches the previous annotation creation.
    assert!(document.undo().unwrap());
    assert_eq!(document.annotations().len(), 1);
}

#[test]
fn core_edit_04_resizing_from_a_corner_keeps_annotations_unchanged_and_is_one_step() {
    let mut document = annotated();
    let version = document.version();
    let start = document.selection().unwrap();
    let corner = start.handle_center(ResizeHandle::BottomRight);
    assert_eq!(corner, [40.0, 25.0]);
    assert_eq!(
        document.begin_drag(corner),
        Some(DragTarget::Resize(ResizeHandle::BottomRight))
    );
    document.update_drag([50.0, 35.0]);
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 40.0, 30.0)));
    // Resizing never scales or moves annotations.
    assert_eq!(
        bounds(&document),
        vec![(12.0, 7.0, 4.0, 4.0), (20.0, 10.0, 8.0, 6.0)]
    );
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);

    assert!(document.undo().unwrap());
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 30.0, 20.0)));
    assert_eq!(
        bounds(&document),
        vec![(12.0, 7.0, 4.0, 4.0), (20.0, 10.0, 8.0, 6.0)]
    );
}

#[test]
fn core_edit_05_all_eight_handles_move_only_their_own_edges() {
    let base = selection(20.0, 10.0, 40.0, 20.0);
    let annotation = selection(25.0, 15.0, 5.0, 5.0);
    let cases = [
        (ResizeHandle::TopLeft, [10.0, 5.0], (10.0, 5.0, 50.0, 25.0)),
        (ResizeHandle::Top, [30.0, 5.0], (20.0, 5.0, 40.0, 25.0)),
        (ResizeHandle::TopRight, [70.0, 5.0], (20.0, 5.0, 50.0, 25.0)),
        (ResizeHandle::Right, [70.0, 20.0], (20.0, 10.0, 50.0, 20.0)),
        (
            ResizeHandle::BottomRight,
            [70.0, 35.0],
            (20.0, 10.0, 50.0, 25.0),
        ),
        (ResizeHandle::Bottom, [40.0, 33.0], (20.0, 10.0, 40.0, 23.0)),
        (
            ResizeHandle::BottomLeft,
            [10.0, 35.0],
            (10.0, 10.0, 50.0, 25.0),
        ),
        (ResizeHandle::Left, [15.0, 20.0], (15.0, 10.0, 45.0, 20.0)),
    ];
    for (handle, target, expected) in cases {
        let mut document = Document::new(frame());
        document.select(base).unwrap();
        document.add_rectangle(annotation).unwrap();
        assert_eq!(
            document.begin_drag(base.handle_center(handle)),
            Some(DragTarget::Resize(handle)),
            "{handle:?}"
        );
        document.update_drag(target);
        let actual = document.selection().unwrap();
        assert_eq!(
            (actual.x(), actual.y(), actual.width(), actual.height()),
            expected,
            "{handle:?}"
        );
        let kept = document.annotations()[0]
            .shape()
            .rectangle_bounds()
            .unwrap();
        assert_eq!(
            (kept.x(), kept.y(), kept.width(), kept.height()),
            (25.0, 15.0, 5.0, 5.0),
            "{handle:?}"
        );
    }
}

#[test]
fn core_edit_06_resize_stops_at_the_screen_edges_and_never_inverts_the_selection() {
    let mut document = annotated();
    let start = document.selection().unwrap();
    document.begin_drag(start.handle_center(ResizeHandle::BottomRight));
    document.update_drag([200.0, 200.0]);
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 90.0, 45.0)));

    // Past the opposite edges the extent stops at one physical pixel at 2x.
    document.update_drag([-20.0, -20.0]);
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 0.5, 0.5)));

    // The drag stays anchored to the baseline edges when it moves back out.
    document.update_drag([50.0, 35.0]);
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 40.0, 30.0)));
    assert!(document.end_drag().unwrap());
}

#[test]
fn core_edit_07_reselect_replaces_selection_and_annotations_but_undo_restores_both() {
    let mut document = annotated();
    document.select(selection(50.0, 20.0, 20.0, 10.0)).unwrap();
    assert_eq!(
        document.selection(),
        Some(selection(50.0, 20.0, 20.0, 10.0))
    );
    assert!(document.annotations().is_empty());

    assert!(document.undo().unwrap());
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 30.0, 20.0)));
    assert_eq!(
        bounds(&document),
        vec![(12.0, 7.0, 4.0, 4.0), (20.0, 10.0, 8.0, 6.0)]
    );

    assert!(document.redo().unwrap());
    assert_eq!(
        document.selection(),
        Some(selection(50.0, 20.0, 20.0, 10.0))
    );
    assert!(document.annotations().is_empty());
}

#[test]
fn core_edit_08_out_of_bounds_annotations_are_retained_and_clipped_then_reappear() {
    let mut document = Document::new(frame());
    document.select(selection(10.0, 10.0, 20.0, 20.0)).unwrap();
    document
        .add_rectangle(selection(25.0, 25.0, 10.0, 10.0))
        .unwrap();
    let annotation = document.annotations()[0].clone();
    let initial = document.selection().unwrap();
    assert_eq!(
        annotation.visible_rectangle(initial),
        Some(selection(25.0, 25.0, 5.0, 5.0))
    );

    // Growing the selection reveals the retained part without changing the data.
    document.begin_drag(initial.handle_center(ResizeHandle::BottomRight));
    document.update_drag([35.0, 35.0]);
    document.end_drag().unwrap();
    let larger = document.selection().unwrap();
    assert_eq!(
        annotation.shape().rectangle_bounds().unwrap(),
        selection(25.0, 25.0, 10.0, 10.0)
    );
    assert_eq!(
        annotation.visible_rectangle(larger),
        Some(selection(25.0, 25.0, 10.0, 10.0))
    );

    // Shrinking until the annotation is fully outside hides it but keeps its data.
    document.begin_drag(larger.handle_center(ResizeHandle::BottomRight));
    document.update_drag([20.0, 20.0]);
    document.end_drag().unwrap();
    let smaller = document.selection().unwrap();
    assert_eq!(smaller, selection(10.0, 10.0, 10.0, 10.0));
    assert_eq!(document.annotations()[0].visible_rectangle(smaller), None);
    assert_eq!(
        document.annotations()[0]
            .shape()
            .rectangle_bounds()
            .unwrap(),
        selection(25.0, 25.0, 10.0, 10.0)
    );
}

#[test]
fn core_edit_09_annotation_layer_order_survives_edits_and_undo() {
    let mut document = annotated();
    let original = bounds(&document);
    assert_eq!(
        original,
        vec![(12.0, 7.0, 4.0, 4.0), (20.0, 10.0, 8.0, 6.0)]
    );

    document.begin_drag([20.0, 10.0]);
    document.update_drag([22.0, 12.0]);
    document.end_drag().unwrap();
    assert_eq!(
        bounds(&document),
        vec![(14.0, 9.0, 4.0, 4.0), (22.0, 12.0, 8.0, 6.0)]
    );

    document.undo().unwrap();
    assert_eq!(bounds(&document), original);
}

#[test]
fn core_edit_10_reselecting_the_same_rectangle_records_nothing() {
    let mut document = Document::new(frame());
    document.select(selection(10.0, 5.0, 30.0, 20.0)).unwrap();
    let version = document.version();
    document.select(selection(10.0, 5.0, 30.0, 20.0)).unwrap();
    assert_eq!(document.version(), version);
    // One undo still reaches the first selection, not a duplicate step.
    assert!(document.undo().unwrap());
    assert_eq!(document.selection(), None);
}

#[test]
fn core_edit_11_edits_and_history_share_the_immutable_base_image() {
    let mut document = annotated();
    let base = document.frame().bgra().as_ptr();
    for step in 0..8 {
        let x = 20.0 + f64::from(step);
        document.select(selection(x, 5.0, 30.0, 20.0)).unwrap();
        document
            .add_rectangle(selection(x + 2.0, 7.0, 4.0, 4.0))
            .unwrap();
        document.undo().unwrap();
        document.redo().unwrap();
    }
    // Every step still points at the same admitted frame allocation.
    assert_eq!(document.frame().bgra().as_ptr(), base);
}

#[test]
fn core_edit_12_rounding_slop_at_the_screen_edge_never_inverts_a_clamp() {
    let mut document = Document::new(frame());
    // The logical conversion accepts a sub-epsilon overshoot at the right edge.
    document
        .select(selection(0.0, 0.0, 100.000_000_5, 50.0))
        .unwrap();

    // Moving a full-screen selection has zero legal range on both axes.
    assert_eq!(document.begin_drag([50.0, 25.0]), Some(DragTarget::Move));
    document.update_drag([80.0, 40.0]);
    assert!(!document.end_drag().unwrap());

    // Resizing the right edge back to the exact screen edge is accepted.
    let start = document.selection().unwrap();
    document.begin_drag(start.handle_center(ResizeHandle::Right));
    document.update_drag([100.0, 25.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.selection(), Some(selection(0.0, 0.0, 100.0, 50.0)));
}
