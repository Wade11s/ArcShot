//! CORE-EDIT: blur rectangle creation, geometry editing, style memory, history.
use arcshot_core::{
    AnnotationStyle, BlurLevel, BlurOption, BlurStyle, CapturedFrame, Document, DragTarget,
    LineOption, PaletteColor, RectangleOption, ResizeHandle, ScreenGeometry, Selection, Tool,
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
    document.set_tool(Tool::Blur);
    assert!(
        document.begin_pointer(from).is_some(),
        "press at {from:?} starts a gesture"
    );
    document.update_drag(to);
    document.end_drag().unwrap();
}

fn bounds(annotation: &arcshot_core::Annotation) -> (f64, f64, f64, f64) {
    let bounds = annotation
        .shape()
        .rectangle_bounds()
        .expect("a blur annotation keeps rectangle bounds");
    (bounds.x(), bounds.y(), bounds.width(), bounds.height())
}

fn blur_style(annotation: &arcshot_core::Annotation) -> BlurStyle {
    annotation
        .style()
        .blur()
        .expect("a blur annotation carries a blur style")
}

#[test]
fn core_edit_32_the_blur_tool_creates_medium_rectangles_from_inside_the_selection() {
    let mut document = selected();
    assert_eq!(document.tool(), Tool::Select);
    document.set_tool(Tool::Blur);
    assert_eq!(document.tool(), Tool::Blur);

    // Creation must start inside the valid selection.
    assert_eq!(document.begin_pointer([5.0, 8.0]), None);
    assert!(document.annotations().is_empty());

    // The retained rectangle may leave the selection; the default is Medium.
    let version = document.version();
    assert_eq!(
        document.begin_pointer([12.0, 8.0]),
        Some(DragTarget::CreateObject)
    );
    document.update_drag([42.0, 20.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(bounds(&document.annotations()[0]), (12.0, 8.0, 30.0, 12.0));
    assert_eq!(
        document.annotations()[0].style(),
        AnnotationStyle::Blur(BlurStyle::default()),
        "a new blur rectangle defaults to the medium level"
    );
    assert_eq!(BlurStyle::default().level(), BlurLevel::Medium);
    // Auto-selected, and the tool stays active for the next gesture.
    assert_eq!(document.selected_index(), Some(0));
    assert_eq!(document.tool(), Tool::Blur);

    draw(&mut document, [24.0, 10.0], [29.0, 15.0]);
    assert_eq!(document.annotations().len(), 2);
    assert_eq!(document.selected_index(), Some(1));
    // Later creations stack above earlier ones in creation order.
    assert_eq!(annotation_at(document.annotations(), [26.0, 12.0]), Some(1));

    // A zero-extent drag creates nothing and records no history step.
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

    // The three levels carry the specified logical-point radii.
    for (level, points) in [
        (BlurLevel::Weak, 4.0),
        (BlurLevel::Medium, 8.0),
        (BlurLevel::Strong, 12.0),
    ] {
        assert_eq!(level.points(), points, "{level:?}");
    }
}

#[test]
fn core_edit_33_blur_options_edit_only_same_type_objects_and_remember_per_session() {
    let mut document = selected();
    assert_eq!(document.blur_style(), BlurStyle::default());
    assert_eq!(
        document.tool_style(Tool::Blur),
        Some(AnnotationStyle::Blur(BlurStyle::default()))
    );

    // With nothing selected only the tool memory changes; no history step.
    let version = document.version();
    assert!(
        !document
            .apply_blur_option(BlurOption::Level(BlurLevel::Weak))
            .unwrap()
    );
    assert_eq!(document.blur_style().level(), BlurLevel::Weak);
    assert_eq!(document.version(), version);

    // A new object keeps the press-time style snapshot; option clicks while a
    // drag is held never split one gesture into two styles.
    document.set_tool(Tool::Blur);
    assert!(document.begin_pointer([12.0, 8.0]).is_some());
    assert!(
        !document
            .apply_blur_option(BlurOption::Level(BlurLevel::Strong))
            .unwrap()
    );
    document.update_drag([30.0, 20.0]);
    document.end_drag().unwrap();
    assert_eq!(
        blur_style(&document.annotations()[0]).level(),
        BlurLevel::Weak
    );

    // A selected blur object takes the option together with the tool memory as
    // exactly one history step; undo restores the object but not the memory.
    let version = document.version();
    assert!(
        document
            .apply_blur_option(BlurOption::Level(BlurLevel::Strong))
            .unwrap()
    );
    assert_eq!(document.version(), version + 1);
    assert_eq!(
        blur_style(&document.annotations()[0]).level(),
        BlurLevel::Strong
    );
    assert_eq!(document.blur_style().level(), BlurLevel::Strong);
    assert!(document.undo().unwrap());
    assert_eq!(
        blur_style(&document.annotations()[0]).level(),
        BlurLevel::Weak
    );
    assert_eq!(document.blur_style().level(), BlurLevel::Strong);
    assert!(document.redo().unwrap());

    // Applying the same level again records nothing.
    let version = document.version();
    assert!(
        !document
            .apply_blur_option(BlurOption::Level(BlurLevel::Strong))
            .unwrap()
    );
    assert_eq!(document.version(), version);

    // Options of another tool never touch a selected blur object.
    document.set_tool(Tool::Select);
    assert_eq!(
        document.begin_pointer([21.0, 14.0]),
        Some(DragTarget::MoveObject)
    );
    document.end_drag().unwrap();
    assert_eq!(document.selected_index(), Some(0));
    let version = document.version();
    assert!(
        !document
            .apply_rectangle_option(RectangleOption::Stroke(PaletteColor::Blue))
            .unwrap()
    );
    assert!(
        !document
            .apply_line_option(LineOption::Width(arcshot_core::StrokeWidth::Two))
            .unwrap()
    );
    assert_eq!(document.version(), version);
    assert_eq!(
        blur_style(&document.annotations()[0]).level(),
        BlurLevel::Strong
    );
    // The rectangle/line tool memories changed; the blur memory did not.
    assert_eq!(document.rectangle_style().stroke(), PaletteColor::Blue);
    assert_eq!(
        document.line_style().width(),
        arcshot_core::StrokeWidth::Two
    );
    assert_eq!(document.blur_style().level(), BlurLevel::Strong);

    // A selected line never takes blur options either.
    document.set_tool(Tool::Line);
    assert!(document.begin_pointer([14.0, 12.0]).is_some());
    document.update_drag([34.0, 12.0]);
    document.end_drag().unwrap();
    let version = document.version();
    assert!(
        !document
            .apply_blur_option(BlurOption::Level(BlurLevel::Weak))
            .unwrap()
    );
    assert_eq!(document.version(), version);
    let drawn = document.annotations()[1]
        .style()
        .line()
        .expect("the second object is a line");
    assert_eq!(drawn.stroke(), PaletteColor::Red);
    assert_eq!(drawn.width(), arcshot_core::StrokeWidth::Two);
    assert!(!drawn.dash());

    // Every new session resets the tool memory to the medium default.
    let fresh = Document::new(frame());
    assert_eq!(fresh.tool(), Tool::Select);
    assert_eq!(fresh.blur_style(), BlurStyle::default());
}

#[test]
fn core_edit_34_blur_geometry_editing_matches_the_rectangle_rules() {
    let mut document = selected();
    draw(&mut document, [12.0, 8.0], [32.0, 22.0]);
    document.set_tool(Tool::Select);

    // Topmost hit, body move as one undoable step.
    let version = document.version();
    assert_eq!(
        document.begin_pointer([22.0, 15.0]),
        Some(DragTarget::MoveObject)
    );
    assert_eq!(document.selected_index(), Some(0));
    document.update_drag([26.0, 18.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(bounds(&document.annotations()[0]), (16.0, 11.0, 20.0, 14.0));
    assert!(document.undo().unwrap());
    assert_eq!(bounds(&document.annotations()[0]), (12.0, 8.0, 20.0, 14.0));

    // Eight control points with edge flip; zero extent keeps the last valid one.
    assert_eq!(
        document.begin_pointer([32.0, 8.0]),
        Some(DragTarget::ResizeObject(ResizeHandle::TopRight))
    );
    document.update_drag([4.0, 24.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(bounds(&document.annotations()[0]), (4.0, 22.0, 8.0, 2.0));

    // Delete removes the object as one step and undo restores it.
    let version = document.version();
    assert!(document.delete_selected().unwrap());
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), version + 1);
    assert!(document.undo().unwrap());
    assert_eq!(document.annotations().len(), 1);

    // Moving the selection carries the blur rectangle by the applied delta.
    // The press point is inside the selection but clear of the object body,
    // its handles and the selection handles.
    let version = document.version();
    assert_eq!(document.begin_pointer([38.0, 10.0]), Some(DragTarget::Move));
    document.update_drag([42.0, 13.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(bounds(&document.annotations()[0]), (8.0, 25.0, 8.0, 2.0));
    assert!(document.undo().unwrap());
    assert_eq!(bounds(&document.annotations()[0]), (4.0, 22.0, 8.0, 2.0));

    // Resizing the selection never scales the retained blur rectangle.
    let selection = document.selection().unwrap();
    assert_eq!(
        document.begin_pointer([
            selection.x() + selection.width(),
            selection.y() + selection.height()
        ]),
        Some(DragTarget::Resize(ResizeHandle::BottomRight))
    );
    document.update_drag([80.0, 45.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(bounds(&document.annotations()[0]), (4.0, 22.0, 8.0, 2.0));
}
