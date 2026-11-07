//! CORE-EDIT: line and arrow creation, hit testing, endpoint edits and styles.
use arcshot_core::{
    AnnotationStyle, ArrowOption, ArrowStyle, CapturedFrame, Document, DragTarget, Endpoint,
    LineOption, LineStyle, PaletteColor, ScreenGeometry, Segment, Selection, StrokeWidth, Tool,
    annotation_at,
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

fn selection(x: f64, y: f64, width: f64, height: f64) -> Selection {
    Selection::try_new(x, y, width, height).unwrap()
}

/// A document with the standard (10,5) 30x20 selection and no objects.
fn selected() -> Document {
    let mut document = Document::new(frame());
    document.select(selection(10.0, 5.0, 30.0, 20.0)).unwrap();
    document
}

fn draw(document: &mut Document, from: [f64; 2], to: [f64; 2]) {
    assert!(
        document.begin_pointer(from).is_some(),
        "press at {from:?} starts a gesture"
    );
    document.update_drag(to);
    document.end_drag().unwrap();
}

fn segment(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
    Segment::try_new([x0, y0], [x1, y1]).unwrap()
}

fn line_style(annotation: &arcshot_core::Annotation) -> LineStyle {
    annotation
        .style()
        .line()
        .expect("a line annotation carries a line style")
}

fn arrow_style(annotation: &arcshot_core::Annotation) -> ArrowStyle {
    annotation
        .style()
        .arrow()
        .expect("an arrow annotation carries an arrow style")
}

#[test]
fn core_edit_25_the_line_tool_creates_directed_segments_from_inside_the_selection() {
    let mut document = selected();
    assert_eq!(document.tool(), Tool::Select);
    document.set_tool(Tool::Line);
    assert_eq!(document.tool(), Tool::Line);

    // Creation always starts inside the valid selection.
    assert_eq!(document.begin_pointer([5.0, 8.0]), None);
    assert!(document.annotations().is_empty());

    // A horizontal line extends past the right selection edge and is retained.
    let version = document.version();
    assert_eq!(
        document.begin_pointer([12.0, 8.0]),
        Some(DragTarget::CreateObject)
    );
    document.update_drag([42.0, 8.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    let line = document.annotations()[0].clone();
    assert_eq!(line.shape().segment(), Some(segment(12.0, 8.0, 42.0, 8.0)));
    assert_eq!(
        line.style(),
        AnnotationStyle::Line(LineStyle::default()),
        "a new line is solid red at three logical points"
    );
    assert_eq!(document.selected_index(), Some(0));
    assert_eq!(document.tool(), Tool::Line);

    // A vertical line has zero width but a non-zero length and is valid.
    draw(&mut document, [20.0, 10.0], [20.0, 20.0]);
    assert_eq!(
        document.annotations()[1].shape().segment(),
        Some(segment(20.0, 10.0, 20.0, 20.0))
    );
    assert_eq!(document.annotations().len(), 2);

    // A zero-length press or a drag that returns to its origin creates nothing.
    let version = document.version();
    document.begin_pointer([15.0, 10.0]);
    document.update_drag([15.0, 10.0]);
    assert!(!document.end_drag().unwrap());
    document.begin_pointer([15.0, 10.0]);
    document.update_drag([25.0, 18.0]);
    document.update_drag([15.0, 10.0]);
    assert!(!document.end_drag().unwrap());
    assert_eq!(document.annotations().len(), 2);
    assert_eq!(document.version(), version);
    assert_eq!(document.selected_index(), None);
}

#[test]
fn core_edit_26_the_arrow_tool_creates_flat_arrows_and_rejects_zero_length() {
    let mut document = selected();
    document.set_tool(Tool::Arrow);
    assert_eq!(document.begin_pointer([5.0, 8.0]), None);

    let version = document.version();
    assert_eq!(
        document.begin_pointer([12.0, 8.0]),
        Some(DragTarget::CreateObject)
    );
    document.update_drag([30.0, 20.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    let arrow = document.annotations()[0].clone();
    // Direction is retained from start tail to end tip.
    assert_eq!(
        arrow.shape().segment(),
        Some(segment(12.0, 8.0, 30.0, 20.0))
    );
    assert_eq!(
        arrow.style(),
        AnnotationStyle::Arrow(ArrowStyle::default()),
        "a new arrow is flat, red and three logical points wide"
    );
    assert_eq!(document.selected_index(), Some(0));
    assert_eq!(document.tool(), Tool::Arrow);

    // Horizontal and vertical arrows are valid even though one extent is zero.
    draw(&mut document, [12.0, 15.0], [38.0, 15.0]);
    draw(&mut document, [25.0, 8.0], [25.0, 20.0]);
    assert_eq!(document.annotations().len(), 3);

    let version = document.version();
    document.begin_pointer([18.0, 12.0]);
    document.update_drag([18.0, 12.0]);
    assert!(!document.end_drag().unwrap());
    assert_eq!(document.annotations().len(), 3);
    assert_eq!(document.version(), version);
}

#[test]
fn core_edit_27_hit_testing_prefers_the_topmost_line_and_tolerates_thin_strokes() {
    let mut document = selected();
    document.set_tool(Tool::Line);
    draw(&mut document, [12.0, 8.0], [28.0, 8.0]);
    draw(&mut document, [12.0, 8.0], [28.0, 8.0]);
    document.set_tool(Tool::Select);
    document.deselect();

    // Coincident layers resolve to the topmost object.
    assert_eq!(annotation_at(document.annotations(), [20.0, 8.0]), Some(1));
    assert_eq!(annotation_at(document.annotations(), [20.0, 30.0]), None);

    // A body press moves the whole line and keeps the selection geometry.
    let version = document.version();
    let active = document.selection();
    assert_eq!(
        document.begin_pointer([20.0, 8.0]),
        Some(DragTarget::MoveObject)
    );
    assert_eq!(document.selected_index(), Some(1));
    document.update_drag([25.0, 10.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(
        document.annotations()[1].shape().segment(),
        Some(segment(17.0, 10.0, 33.0, 10.0))
    );
    assert_eq!(document.selection(), active);
    assert!(document.undo().unwrap());
    assert_eq!(
        document.annotations()[1].shape().segment(),
        Some(segment(12.0, 8.0, 28.0, 8.0))
    );

    // A press on an endpoint is an endpoint edit, not a body move.
    assert_eq!(
        document.begin_pointer([28.0, 8.0]),
        Some(DragTarget::MoveEndpoint(Endpoint::End))
    );
    assert!(!document.end_drag().unwrap());
    assert_eq!(
        document.begin_pointer([12.0, 8.0]),
        Some(DragTarget::MoveEndpoint(Endpoint::Start))
    );
    assert!(!document.end_drag().unwrap());

    // A two point stroke is still selectable a few points away from the path.
    let mut thin = selected();
    thin.set_tool(Tool::Line);
    thin.apply_line_option(LineOption::Width(StrokeWidth::Two))
        .unwrap();
    draw(&mut thin, [12.0, 10.0], [28.0, 10.0]);
    thin.set_tool(Tool::Select);
    thin.deselect();
    assert_eq!(annotation_at(thin.annotations(), [20.0, 12.5]), Some(0));
    assert_eq!(annotation_at(thin.annotations(), [20.0, 14.5]), None);
}

#[test]
fn core_edit_28_endpoints_edit_length_and_direction_and_may_coincide() {
    let mut document = selected();
    document.set_tool(Tool::Line);
    draw(&mut document, [12.0, 8.0], [28.0, 8.0]);
    document.set_tool(Tool::Select);

    let version = document.version();
    assert_eq!(
        document.begin_pointer([28.0, 8.0]),
        Some(DragTarget::MoveEndpoint(Endpoint::End))
    );
    document.update_drag([28.0, 18.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(
        document.annotations()[0].shape().segment(),
        Some(segment(12.0, 8.0, 28.0, 18.0))
    );
    assert_eq!(
        document.begin_pointer([28.0, 18.0]),
        Some(DragTarget::MoveEndpoint(Endpoint::End))
    );
    document.update_drag([28.0, 18.0]);
    assert!(
        !document.end_drag().unwrap(),
        "an unchanged endpoint is no-op"
    );
    assert_eq!(document.version(), version + 1);

    // Undo/redo restore the committed endpoint geometry.
    assert!(document.undo().unwrap());
    assert_eq!(
        document.annotations()[0].shape().segment(),
        Some(segment(12.0, 8.0, 28.0, 8.0))
    );
    assert!(document.redo().unwrap());
    assert_eq!(
        document.annotations()[0].shape().segment(),
        Some(segment(12.0, 8.0, 28.0, 18.0))
    );

    // An existing endpoint may be dragged onto the other one without deleting
    // the object; only a new zero-length stroke is rejected.
    document.begin_pointer([28.0, 18.0]);
    document.update_drag([12.0, 8.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.annotations().len(), 1);
    let collapsed = document.annotations()[0].shape().segment().unwrap();
    assert_eq!(collapsed.start(), [12.0, 8.0]);
    assert_eq!(collapsed.end(), [12.0, 8.0]);
    assert!(collapsed.is_zero());
    assert!(!document.dragging());

    let version = document.version();
    document.set_tool(Tool::Line);
    document.begin_pointer([15.0, 10.0]);
    document.update_drag([15.0, 10.0]);
    assert!(!document.end_drag().unwrap());
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(document.version(), version);
}

#[test]
fn core_edit_29_line_and_arrow_options_edit_only_same_type_objects() {
    let mut document = selected();
    assert_eq!(document.line_style(), LineStyle::default());
    assert_eq!(document.arrow_style(), ArrowStyle::default());
    assert_eq!(
        document.tool_style(Tool::Line),
        Some(AnnotationStyle::Line(LineStyle::default()))
    );
    assert_eq!(
        document.tool_style(Tool::Arrow),
        Some(AnnotationStyle::Arrow(ArrowStyle::default()))
    );

    // With nothing selected only the line tool memory changes; no history step.
    let version = document.version();
    assert!(!document.apply_line_option(LineOption::Dash(true)).unwrap());
    assert!(document.line_style().dash());
    assert_eq!(document.version(), version);

    // A selected line takes the option together with the tool memory.
    document.set_tool(Tool::Line);
    draw(&mut document, [12.0, 8.0], [28.0, 8.0]);
    assert!(line_style(&document.annotations()[0]).dash());
    let version = document.version();
    assert!(
        document
            .apply_line_option(LineOption::Width(StrokeWidth::Five))
            .unwrap()
    );
    assert_eq!(document.version(), version + 1);
    assert_eq!(
        line_style(&document.annotations()[0]).width(),
        StrokeWidth::Five
    );
    assert_eq!(document.line_style().width(), StrokeWidth::Five);
    // Undo restores the object but not the session style memory.
    assert!(document.undo().unwrap());
    assert_eq!(
        line_style(&document.annotations()[0]).width(),
        StrokeWidth::Three
    );
    assert_eq!(document.line_style().width(), StrokeWidth::Five);
    assert!(document.redo().unwrap());

    // Selecting the line and applying an arrow option leaves it untouched.
    document.set_tool(Tool::Select);
    assert_eq!(
        document.begin_pointer([20.0, 8.0]),
        Some(DragTarget::MoveObject)
    );
    document.end_drag().unwrap();
    assert_eq!(document.selected_index(), Some(0));
    let version = document.version();
    assert!(
        !document
            .apply_arrow_option(ArrowOption::Inward(true))
            .unwrap()
    );
    assert_eq!(document.version(), version);
    assert!(document.arrow_style().inward());
    assert_eq!(
        line_style(&document.annotations()[0]).width(),
        StrokeWidth::Five
    );

    // A selected arrow takes arrow options and keeps line options separate.
    document.set_tool(Tool::Arrow);
    draw(&mut document, [14.0, 12.0], [34.0, 12.0]);
    assert!(arrow_style(&document.annotations()[1]).inward());
    let version = document.version();
    assert!(
        document
            .apply_arrow_option(ArrowOption::Width(StrokeWidth::Two))
            .unwrap()
    );
    assert_eq!(document.version(), version + 1);
    assert_eq!(
        arrow_style(&document.annotations()[1]).width(),
        StrokeWidth::Two
    );
    assert_eq!(document.arrow_style().width(), StrokeWidth::Two);
    assert_eq!(document.line_style().width(), StrokeWidth::Five);
    // Applying the same value again records nothing.
    let version = document.version();
    assert!(
        !document
            .apply_arrow_option(ArrowOption::Width(StrokeWidth::Two))
            .unwrap()
    );
    assert_eq!(document.version(), version);
}

#[test]
fn core_edit_30_creation_keeps_the_press_style_and_new_sessions_reset_defaults() {
    let mut document = selected();
    document.set_tool(Tool::Line);
    document
        .apply_line_option(LineOption::Stroke(PaletteColor::Blue))
        .unwrap();
    document
        .apply_line_option(LineOption::Width(StrokeWidth::Five))
        .unwrap();
    document.apply_line_option(LineOption::Dash(true)).unwrap();

    assert_eq!(
        document.begin_pointer([12.0, 8.0]),
        Some(DragTarget::CreateObject)
    );
    // Options are ignored while a drag is held, so one gesture keeps one style.
    assert!(
        !document
            .apply_line_option(LineOption::Stroke(PaletteColor::Green))
            .unwrap()
    );
    document.update_drag([30.0, 8.0]);
    document.end_drag().unwrap();
    let style = line_style(&document.annotations()[0]);
    assert_eq!(style.stroke(), PaletteColor::Blue);
    assert_eq!(style.width(), StrokeWidth::Five);
    assert!(style.dash());
    assert_eq!(document.line_style().stroke(), PaletteColor::Blue);

    let fresh = Document::new(frame());
    assert_eq!(fresh.tool(), Tool::Select);
    assert_eq!(fresh.line_style(), LineStyle::default());
    assert_eq!(fresh.arrow_style(), ArrowStyle::default());
    assert_eq!(fresh.line_style().stroke(), PaletteColor::Red);
    assert_eq!(fresh.line_style().width(), StrokeWidth::Three);
    assert!(!fresh.line_style().dash());
    assert_eq!(fresh.arrow_style().stroke(), PaletteColor::Red);
    assert_eq!(fresh.arrow_style().width(), StrokeWidth::Three);
    assert!(!fresh.arrow_style().inward());
}

#[test]
fn core_edit_31_selection_move_carries_line_endpoints_and_resize_never_scales_them() {
    let mut document = selected();
    document.set_tool(Tool::Line);
    draw(&mut document, [12.0, 8.0], [28.0, 8.0]);
    document.set_tool(Tool::Select);

    // Resizing the selection leaves the line geometry untouched.
    assert_eq!(
        document.begin_pointer([40.0, 25.0]),
        Some(DragTarget::Resize(arcshot_core::ResizeHandle::BottomRight))
    );
    document.update_drag([60.0, 35.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 50.0, 30.0)));
    assert_eq!(
        document.annotations()[0].shape().segment(),
        Some(segment(12.0, 8.0, 28.0, 8.0))
    );

    // Moving the selection carries every endpoint by the applied delta.
    let version = document.version();
    assert_eq!(document.begin_pointer([55.0, 20.0]), Some(DragTarget::Move));
    document.update_drag([60.0, 22.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.selection(), Some(selection(15.0, 7.0, 50.0, 30.0)));
    assert_eq!(
        document.annotations()[0].shape().segment(),
        Some(segment(17.0, 10.0, 33.0, 10.0))
    );
    assert!(document.undo().unwrap());
    assert_eq!(
        document.annotations()[0].shape().segment(),
        Some(segment(12.0, 8.0, 28.0, 8.0))
    );
}
