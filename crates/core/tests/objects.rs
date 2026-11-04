//! CORE-EDIT: rectangle creation, hit testing, object edits, style and history.
use arcshot_core::{
    CapturedFrame, Document, DragTarget, PaletteColor, RectangleOption, RectangleStyle,
    ResizeHandle, ScreenGeometry, Selection, StrokeWidth, Tool, annotation_at,
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
    document.set_tool(Tool::Rectangle);
    assert!(document.begin_pointer(from).is_some(), "press at {from:?}");
    document.update_drag(to);
    document.end_drag().unwrap();
}

fn bounds(annotation: arcshot_core::Annotation) -> (f64, f64, f64, f64) {
    let bounds = annotation.shape().rectangle_bounds().unwrap();
    (bounds.x(), bounds.y(), bounds.width(), bounds.height())
}

#[test]
fn core_edit_13_the_rectangle_tool_creates_one_selected_object_per_gesture() {
    let mut document = selected();
    assert_eq!(document.tool(), Tool::Select);
    document.set_tool(Tool::Rectangle);
    assert_eq!(document.tool(), Tool::Rectangle);

    // Creation must start inside the valid selection.
    assert_eq!(document.begin_pointer([5.0, 5.0]), None);
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), 1);

    let version = document.version();
    assert_eq!(
        document.begin_pointer([12.0, 8.0]),
        Some(DragTarget::CreateObject)
    );
    document.update_drag([22.0, 18.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(bounds(document.annotations()[0]), (12.0, 8.0, 10.0, 10.0));
    assert_eq!(
        document.annotations()[0].style().rectangle(),
        Some(RectangleStyle::default())
    );
    // Auto-selected, and the tool stays active for the next gesture.
    assert_eq!(document.selected_index(), Some(0));
    assert_eq!(document.tool(), Tool::Rectangle);

    assert!(document.begin_pointer([24.0, 10.0]).is_some());
    document.update_drag([29.0, 15.0]);
    document.end_drag().unwrap();
    assert_eq!(document.annotations().len(), 2);
    assert_eq!(document.selected_index(), Some(1));
    // Later creations stack above earlier ones in creation order.
    assert_eq!(annotation_at(document.annotations(), [26.0, 12.0]), Some(1));
}

#[test]
fn core_edit_14_zero_width_or_height_creates_nothing_and_no_history_step() {
    let mut document = selected();
    document.set_tool(Tool::Rectangle);
    let version = document.version();

    // A zero-width drag creates nothing.
    document.begin_pointer([12.0, 8.0]);
    document.update_drag([12.0, 18.0]);
    assert!(!document.end_drag().unwrap());
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), version);

    // A valid draft dragged back to a zero extent creates nothing either.
    document.begin_pointer([12.0, 8.0]);
    document.update_drag([22.0, 18.0]);
    document.update_drag([22.0, 8.0]);
    assert!(!document.end_drag().unwrap());
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), version);

    // A plain click is the zero-drag case.
    document.begin_pointer([15.0, 10.0]);
    assert!(!document.end_drag().unwrap());
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), version);
}

#[test]
fn core_edit_15_creation_starts_inside_the_selection_but_may_extend_outside() {
    let mut document = selected();
    document.set_tool(Tool::Rectangle);
    // A press that starts outside cannot begin even if the drag would enter.
    assert_eq!(document.begin_pointer([5.0, 8.0]), None);
    assert!(document.annotations().is_empty());

    document.begin_pointer([12.0, 8.0]);
    document.update_drag([55.0, 45.0]);
    document.end_drag().unwrap();
    let created = document.annotations()[0];
    assert_eq!(bounds(created), (12.0, 8.0, 43.0, 37.0));
    // Retained data leaves the selection; preview and output clip it.
    let active = document.selection().unwrap();
    assert_eq!(
        created.visible_rectangle(active),
        Some(selection(12.0, 8.0, 28.0, 17.0))
    );
}

#[test]
fn core_edit_16_the_topmost_object_wins_and_object_drags_precede_selection_drags() {
    let mut document = selected();
    // Two overlapping objects; the second is above the first.
    draw(&mut document, [12.0, 8.0], [22.0, 18.0]);
    draw(&mut document, [15.0, 10.0], [25.0, 20.0]);
    assert_eq!(annotation_at(document.annotations(), [20.0, 15.0]), Some(1));
    assert_eq!(annotation_at(document.annotations(), [12.5, 8.5]), Some(0));
    assert_eq!(annotation_at(document.annotations(), [35.0, 22.0]), None);

    document.set_tool(Tool::Select);
    let selection = document.selection().unwrap();
    assert_eq!(
        document.begin_pointer([20.0, 15.0]),
        Some(DragTarget::MoveObject)
    );
    document.update_drag([23.0, 18.0]);
    document.end_drag().unwrap();
    assert_eq!(document.selected_index(), Some(1));
    assert_eq!(bounds(document.annotations()[1]), (18.0, 13.0, 10.0, 10.0));
    assert_eq!(bounds(document.annotations()[0]), (12.0, 8.0, 10.0, 10.0));
    // The whole selection never moved: the object drag had priority.
    assert_eq!(document.selection(), Some(selection));
}

#[test]
fn core_edit_17_object_handles_resize_one_edge_and_may_flip_across_the_opposite_edge() {
    let base = selection(20.0, 10.0, 10.0, 5.0);
    let cases = [
        (ResizeHandle::TopLeft, [15.0, 8.0], (15.0, 8.0, 15.0, 7.0)),
        (ResizeHandle::Top, [25.0, 7.0], (20.0, 7.0, 10.0, 8.0)),
        (ResizeHandle::TopRight, [35.0, 7.0], (20.0, 7.0, 15.0, 8.0)),
        (ResizeHandle::Right, [33.0, 12.0], (20.0, 10.0, 13.0, 5.0)),
        (
            ResizeHandle::BottomRight,
            [33.0, 18.0],
            (20.0, 10.0, 13.0, 8.0),
        ),
        (ResizeHandle::Bottom, [24.0, 18.0], (20.0, 10.0, 10.0, 8.0)),
        (
            ResizeHandle::BottomLeft,
            [15.0, 18.0],
            (15.0, 10.0, 15.0, 8.0),
        ),
        (ResizeHandle::Left, [17.0, 12.0], (17.0, 10.0, 13.0, 5.0)),
    ];
    for (handle, target, expected) in cases {
        let mut document = selected();
        draw(&mut document, [base.x(), base.y()], [30.0, 15.0]);
        document.set_tool(Tool::Select);
        let object = document.annotations()[0]
            .shape()
            .rectangle_bounds()
            .unwrap();
        assert_eq!(
            document.begin_pointer(object.handle_center(handle)),
            Some(DragTarget::ResizeObject(handle)),
            "{handle:?}"
        );
        document.update_drag(target);
        assert_eq!(bounds(document.annotations()[0]), expected, "{handle:?}");
        assert!(document.end_drag().unwrap());
    }

    // Dragging the top-left corner past the bottom-right flips both axes.
    let mut document = selected();
    draw(&mut document, [20.0, 10.0], [30.0, 15.0]);
    document.set_tool(Tool::Select);
    let version = document.version();
    let object = document.annotations()[0]
        .shape()
        .rectangle_bounds()
        .unwrap();
    assert_eq!(
        document.begin_pointer(object.handle_center(ResizeHandle::TopLeft)),
        Some(DragTarget::ResizeObject(ResizeHandle::TopLeft))
    );
    document.update_drag([35.0, 20.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(bounds(document.annotations()[0]), (30.0, 15.0, 5.0, 5.0));
    assert_eq!(document.version(), version + 1);
    assert!(document.undo().unwrap());
    assert_eq!(bounds(document.annotations()[0]), (20.0, 10.0, 10.0, 5.0));
}

#[test]
fn core_edit_18_a_click_on_empty_space_deselects_but_a_drag_moves_the_selection_and_objects() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [16.0, 12.0]);
    document.set_tool(Tool::Select);
    assert_eq!(document.selected_index(), Some(0));

    // Pressing empty space clears the object selection and starts a selection move.
    assert_eq!(document.begin_pointer([35.0, 22.0]), Some(DragTarget::Move));
    assert_eq!(document.selected_index(), None);
    document.update_drag([37.0, 23.0]);
    // The live preview carries every object with the selection.
    assert_eq!(document.selection(), Some(selection(12.0, 6.0, 30.0, 20.0)));
    assert_eq!(bounds(document.annotations()[0]), (14.0, 9.0, 4.0, 4.0));
    assert!(document.end_drag().unwrap());
    assert!(document.undo().unwrap());
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 30.0, 20.0)));
    assert_eq!(bounds(document.annotations()[0]), (12.0, 8.0, 4.0, 4.0));
}

#[test]
fn core_edit_19_delete_removes_only_the_selected_object_as_one_undoable_step() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [16.0, 12.0]);
    draw(&mut document, [18.0, 10.0], [22.0, 14.0]);
    document.set_tool(Tool::Select);
    // Select the lower object, delete it, then restore the layer order by undo.
    assert!(document.begin_pointer([14.0, 10.0]).is_some());
    assert!(!document.end_drag().unwrap());
    assert_eq!(document.selected_index(), Some(0));
    let version = document.version();
    assert!(document.delete_selected().unwrap());
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(bounds(document.annotations()[0]), (18.0, 10.0, 4.0, 4.0));
    assert_eq!(document.selected_index(), None);
    assert_eq!(document.version(), version + 1);

    assert!(document.undo().unwrap());
    assert_eq!(document.annotations().len(), 2);
    assert_eq!(bounds(document.annotations()[0]), (12.0, 8.0, 4.0, 4.0));
    assert_eq!(bounds(document.annotations()[1]), (18.0, 10.0, 4.0, 4.0));
    assert!(document.redo().unwrap());
    assert_eq!(document.annotations().len(), 1);

    // Deleting with no selected object is a no-op.
    let version = document.version();
    assert!(!document.delete_selected().unwrap());
    assert_eq!(document.version(), version);
}

#[test]
fn core_edit_20_style_options_change_the_selected_object_and_the_tool_memory() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [16.0, 12.0]);
    draw(&mut document, [20.0, 10.0], [24.0, 14.0]);
    document.set_tool(Tool::Select);
    assert!(document.begin_pointer([14.0, 10.0]).is_some());
    document.end_drag().unwrap();
    assert_eq!(document.selected_index(), Some(0));

    // With the first object selected, the option changes that object and the memory.
    let version = document.version();
    assert!(
        document
            .apply_rectangle_option(RectangleOption::Width(StrokeWidth::Five))
            .unwrap()
    );
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.annotations()[0].style().width(), StrokeWidth::Five);
    assert_eq!(
        document.annotations()[1].style().width(),
        StrokeWidth::Three
    );
    assert_eq!(document.rectangle_style().width(), StrokeWidth::Five);

    // Undo restores the object style; the tool's current style is not history.
    assert!(document.undo().unwrap());
    assert_eq!(
        document.annotations()[0].style().width(),
        StrokeWidth::Three
    );
    assert_eq!(document.rectangle_style().width(), StrokeWidth::Five);
    assert!(document.redo().unwrap());
    assert_eq!(document.annotations()[0].style().width(), StrokeWidth::Five);

    // Re-selecting and choosing the same value changes nothing.
    document.begin_pointer([14.0, 10.0]);
    document.end_drag().unwrap();
    let version = document.version();
    assert!(
        !document
            .apply_rectangle_option(RectangleOption::Width(StrokeWidth::Five))
            .unwrap()
    );
    assert_eq!(document.version(), version);

    // With nothing selected, only the tool memory changes and no object is touched.
    assert!(document.deselect());
    let version = document.version();
    document
        .apply_rectangle_option(RectangleOption::Stroke(PaletteColor::Blue))
        .unwrap();
    document
        .apply_rectangle_option(RectangleOption::Fill(true))
        .unwrap();
    assert_eq!(document.rectangle_style().stroke(), PaletteColor::Blue);
    assert!(document.rectangle_style().fill());
    assert_eq!(document.version(), version);
    for annotation in document.annotations() {
        assert_eq!(annotation.style().stroke(), PaletteColor::Red);
        assert!(!annotation.style().rectangle().unwrap().fill());
    }
}

#[test]
fn core_edit_21_each_tool_keeps_its_style_for_the_session_and_new_documents_default() {
    let mut document = selected();
    document.set_tool(Tool::Rectangle);
    document
        .apply_rectangle_option(RectangleOption::Stroke(PaletteColor::Green))
        .unwrap();
    document
        .apply_rectangle_option(RectangleOption::Width(StrokeWidth::Two))
        .unwrap();
    document
        .apply_rectangle_option(RectangleOption::Fill(true))
        .unwrap();
    document.begin_pointer([12.0, 8.0]);
    document.update_drag([18.0, 14.0]);
    document.end_drag().unwrap();
    let style = document.annotations()[0].style();
    assert_eq!(style.stroke(), PaletteColor::Green);
    assert_eq!(style.width(), StrokeWidth::Two);
    assert!(style.rectangle().unwrap().fill());

    // A new session's document starts again from the Rectangle defaults.
    let fresh = Document::new(frame());
    assert_eq!(fresh.tool(), Tool::Select);
    assert_eq!(fresh.rectangle_style(), RectangleStyle::default());
    assert_eq!(fresh.rectangle_style().stroke(), PaletteColor::Red);
    assert_eq!(fresh.rectangle_style().width(), StrokeWidth::Three);
    assert!(!fresh.rectangle_style().fill());
}

#[test]
fn core_edit_22_each_object_drag_is_one_history_step_and_no_ops_record_nothing() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [18.0, 14.0]);
    document.set_tool(Tool::Select);
    let version = document.version();
    assert_eq!(
        document.begin_pointer([15.0, 11.0]),
        Some(DragTarget::MoveObject)
    );
    document.update_drag([15.0, 11.0]);
    assert!(!document.end_drag().unwrap());
    assert_eq!(document.version(), version);

    assert!(document.begin_pointer([15.0, 11.0]).is_some());
    document.update_drag([18.0, 14.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(bounds(document.annotations()[0]), (15.0, 11.0, 6.0, 6.0));
    assert!(document.undo().unwrap());
    assert_eq!(bounds(document.annotations()[0]), (12.0, 8.0, 6.0, 6.0));
    assert!(document.redo().unwrap());
    assert_eq!(bounds(document.annotations()[0]), (15.0, 11.0, 6.0, 6.0));
}

#[test]
fn core_edit_23_an_object_may_leave_the_selection_and_keeps_its_data_until_undo() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [17.0, 13.0]);
    document.set_tool(Tool::Select);
    assert!(document.begin_pointer([14.0, 10.0]).is_some());
    document.update_drag([44.0, 30.0]);
    document.end_drag().unwrap();
    let moved = document.annotations()[0];
    assert_eq!(bounds(moved), (42.0, 28.0, 5.0, 5.0));
    let active = document.selection().unwrap();
    assert_eq!(moved.visible_rectangle(active), None);
    assert_eq!(
        moved.shape().rectangle_bounds().unwrap(),
        selection(42.0, 28.0, 5.0, 5.0)
    );
    assert!(document.undo().unwrap());
    assert_eq!(bounds(document.annotations()[0]), (12.0, 8.0, 5.0, 5.0));
}

#[test]
fn core_edit_24_selecting_a_tool_or_deselecting_is_not_a_history_step() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [16.0, 12.0]);
    let version = document.version();
    assert!(document.deselect());
    assert_eq!(document.selected_index(), None);
    assert!(!document.deselect());
    document.set_tool(Tool::Rectangle);
    document.set_tool(Tool::Select);
    assert_eq!(document.version(), version);
    // Selection state is not part of history; undo still restores object edits.
    assert!(document.undo().unwrap());
    assert!(document.annotations().is_empty());
}
