//! CORE-EDIT / CORE-SESSION: text creation, re-editing, size, history and fences.
use arcshot_core::{
    AnnotationStyle, CaptureAction, CaptureEffect, CapturePhase, CaptureSession, CaptureTicket,
    CapturedFrame, Document, PaletteColor, ScreenGeometry, Selection, TextOption, TextStyle, Tool,
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

fn bounds_for(anchor: [f64; 2], width: f64, height: f64) -> Selection {
    selection(anchor[0], anchor[1], width, height)
}

/// Creates one committed text object and returns its index.
fn create_text(document: &mut Document, content: &str) -> usize {
    document.set_tool(Tool::Text);
    assert!(document.begin_text_edit([12.0, 8.0]));
    document.update_text_edit(content, bounds_for([12.0, 8.0], 20.0, 16.0));
    assert!(document.commit_edits().unwrap());
    document.annotations().len() - 1
}

#[test]
fn core_edit_35_text_creation_is_one_step_and_empty_content_is_dropped() {
    let mut document = selected();
    assert_eq!(document.tool(), Tool::Select);
    assert_eq!(document.text_style(), TextStyle::default());
    assert_eq!(TextStyle::default().color(), PaletteColor::Red);
    assert_eq!(TextStyle::default().size_points(), 16.0);

    // Creation must start inside the valid selection with the text tool.
    document.set_tool(Tool::Text);
    assert!(!document.begin_text_edit([5.0, 8.0]));
    assert!(document.text_edit().is_none());
    assert!(document.begin_text_edit([12.0, 8.0]));
    let edit = document.text_edit().expect("an edit is pending");
    assert_eq!(edit.target(), None);
    assert_eq!(edit.anchor(), [12.0, 8.0]);
    assert_eq!(edit.content(), "");
    assert_eq!(edit.style(), TextStyle::default());

    // Empty and whitespace-only drafts leave no object and no history step.
    let version = document.version();
    assert!(!document.commit_edits().unwrap());
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), version);
    assert!(document.text_edit().is_none());

    assert!(document.begin_text_edit([12.0, 8.0]));
    document.update_text_edit("   \n  ", bounds_for([12.0, 8.0], 1.0, 1.0));
    assert!(!document.commit_edits().unwrap());
    assert!(document.annotations().is_empty());
    assert_eq!(document.version(), version);

    // Real content is one creation step and is auto-selected.
    assert!(document.begin_text_edit([12.0, 8.0]));
    document.update_text_edit("Hello", bounds_for([12.0, 8.0], 20.0, 16.0));
    assert!(document.commit_edits().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(document.selected_index(), Some(0));
    assert_eq!(document.tool(), Tool::Text, "the text tool stays active");
    let annotation = document.annotations()[0].clone();
    assert_eq!(
        annotation.style(),
        AnnotationStyle::Text(TextStyle::default())
    );
    let text = annotation.shape().text().expect("a text object");
    assert_eq!(text.content(), "Hello");
    assert_eq!(text.anchor(), [12.0, 8.0]);
    assert_eq!(text.bounds(), bounds_for([12.0, 8.0], 20.0, 16.0));

    // The creation is one undoable step.
    assert!(document.undo().unwrap());
    assert!(document.annotations().is_empty());
    assert!(document.redo().unwrap());
    assert_eq!(document.annotations().len(), 1);
}

#[test]
fn core_edit_36_re_editing_is_one_modification_and_empty_deletes() {
    let mut document = selected();
    create_text(&mut document, "First");
    let version = document.version();
    document.set_tool(Tool::Select);

    // A select-tool press on the text body starts a re-edit of that object.
    assert!(document.begin_text_edit([14.0, 12.0]));
    let edit = document.text_edit().expect("a re-edit is pending");
    assert_eq!(edit.target(), Some(0));
    assert_eq!(edit.content(), "First");
    assert_eq!(edit.style(), TextStyle::default());

    // Typing a new value and committing is exactly one modification step.
    document.update_text_edit("Second", bounds_for([12.0, 8.0], 24.0, 16.0));
    assert!(document.commit_edits().unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(
        document.annotations()[0].shape().text().unwrap().content(),
        "Second"
    );
    assert!(document.undo().unwrap());
    assert_eq!(
        document.annotations()[0].shape().text().unwrap().content(),
        "First"
    );
    assert!(document.redo().unwrap());

    // Editing an existing object to empty deletes it as one undoable step.
    assert!(document.begin_text_edit([14.0, 12.0]));
    document.update_text_edit("  ", bounds_for([12.0, 8.0], 1.0, 1.0));
    assert!(document.commit_edits().unwrap());
    assert_eq!(document.version(), version + 4);
    assert!(document.annotations().is_empty());
    assert!(document.undo().unwrap());
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(
        document.annotations()[0].shape().text().unwrap().content(),
        "Second"
    );
}

#[test]
fn core_edit_37_text_size_is_clamped_and_quantized() {
    assert_eq!(TextStyle::quantize_size(5.0), 6.0);
    assert_eq!(TextStyle::quantize_size(5.9), 6.0);
    assert_eq!(TextStyle::quantize_size(64.4), 64.0);
    assert_eq!(TextStyle::quantize_size(16.2), 16.0);
    assert_eq!(TextStyle::quantize_size(16.25), 16.5);
    assert_eq!(TextStyle::quantize_size(16.49), 16.5);
    assert_eq!(TextStyle::quantize_size(f64::NAN), 16.0);
    assert_eq!(TextStyle::quantize_size(f64::INFINITY), 64.0);

    let mut document = selected();
    create_text(&mut document, "Size");
    let version = document.version();

    // With nothing selected only the tool memory changes.
    document.set_tool(Tool::Select);
    document.deselect();
    assert!(!document.apply_text_option(TextOption::Size(12.3)).unwrap());
    assert_eq!(document.text_style().size_points(), 12.5);
    assert_eq!(document.version(), version);

    // A selected text object takes the option together with the memory.
    document.set_tool(Tool::Select);
    assert_eq!(
        document.begin_pointer([14.0, 12.0]),
        Some(arcshot_core::DragTarget::MoveObject)
    );
    document.end_drag().unwrap();
    assert!(document.apply_text_option(TextOption::Size(20.0)).unwrap());
    assert_eq!(document.version(), version + 1);
    assert_eq!(
        document.annotations()[0]
            .style()
            .text()
            .unwrap()
            .size_points(),
        20.0
    );
    assert_eq!(document.text_style().size_points(), 20.0);
    assert!(document.undo().unwrap());
    assert_eq!(
        document.annotations()[0]
            .style()
            .text()
            .unwrap()
            .size_points(),
        16.0
    );
    assert_eq!(document.text_style().size_points(), 20.0);

    // Colour and size options of another tool never touch a text object.
    document.set_tool(Tool::Rectangle);
    let version = document.version();
    assert!(
        !document
            .apply_rectangle_option(arcshot_core::RectangleOption::Stroke(PaletteColor::Blue))
            .unwrap()
    );
    assert_eq!(document.version(), version);
}

#[test]
fn core_edit_38_text_hits_moves_and_stacks() {
    let mut document = selected();
    create_text(&mut document, "Lower");
    create_text(&mut document, "Upper");
    assert_eq!(document.annotations().len(), 2);
    // Later creation stacks above the earlier one.
    assert_eq!(annotation_at(document.annotations(), [14.0, 12.0]), Some(1));
    assert!(document.annotations()[0].shape().text().is_some());
    assert!(
        document.annotations()[0]
            .resize_handle_at([14.0, 12.0])
            .is_none()
    );
    assert!(
        document.annotations()[0]
            .endpoint_at([14.0, 12.0])
            .is_none()
    );
    assert!(document.annotations()[0].contains([14.0, 12.0]));

    // The select tool moves a text body as one undoable step.
    document.set_tool(Tool::Select);
    let version = document.version();
    assert_eq!(
        document.begin_pointer([14.0, 12.0]),
        Some(arcshot_core::DragTarget::MoveObject)
    );
    document.update_drag([18.0, 16.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    let moved = document.annotations()[1].shape().text().unwrap().clone();
    assert_eq!(moved.anchor(), [16.0, 12.0]);
    assert_eq!(moved.bounds(), bounds_for([16.0, 12.0], 20.0, 16.0));
    assert!(document.undo().unwrap());

    // Moving the selection carries the text by the applied delta.
    let version = document.version();
    assert_eq!(
        document.begin_pointer([38.0, 10.0]),
        Some(arcshot_core::DragTarget::Move)
    );
    document.update_drag([42.0, 13.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.version(), version + 1);
    let carried = document.annotations()[1].shape().text().unwrap().clone();
    assert_eq!(carried.anchor(), [16.0, 11.0]);
}

#[test]
fn core_edit_39_a_text_edit_commits_when_the_tool_changes() {
    let mut document = selected();
    document.set_tool(Tool::Text);
    assert!(document.begin_text_edit([12.0, 8.0]));
    document.update_text_edit("Pending", bounds_for([12.0, 8.0], 20.0, 16.0));
    let version = document.version();
    document.set_tool(Tool::Select);
    assert_eq!(document.version(), version + 1);
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(document.tool(), Tool::Select);
    assert!(document.text_edit().is_none());
    assert!(document.undo().unwrap());
    assert!(document.annotations().is_empty());
}

fn frozen() -> (CaptureSession, CaptureTicket) {
    let mut session = CaptureSession::default();
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    let effects = session.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    assert_eq!(effects, vec![CaptureEffect::PresentFrozen(ticket)]);
    (session, ticket)
}

fn select(session: &mut CaptureSession, ticket: CaptureTicket) {
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
}

#[test]
fn core_session_28_text_actions_share_the_edit_fence() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let version = session.document().unwrap().version();

    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Text,
    });
    session.dispatch(CaptureAction::TextBegin {
        ticket,
        point: [12.0, 8.0],
    });
    session.dispatch(CaptureAction::TextEdited {
        ticket,
        content: "Hello".to_owned(),
        bounds: selection(12.0, 8.0, 20.0, 16.0),
    });
    // The draft is not an annotation yet; the version is unchanged.
    assert_eq!(session.document().unwrap().annotations().len(), 0);
    assert_eq!(session.document().unwrap().version(), version);

    // A text option edits the draft and the tool memory without history.
    session.dispatch(CaptureAction::TextOption {
        ticket,
        option: TextOption::Size(18.0),
    });
    assert_eq!(
        session
            .document()
            .unwrap()
            .text_edit()
            .unwrap()
            .style()
            .size_points(),
        18.0
    );
    assert_eq!(session.document().unwrap().version(), version);

    session.dispatch(CaptureAction::TextCommit { ticket });
    assert_eq!(session.document().unwrap().version(), version + 1);
    assert_eq!(session.document().unwrap().annotations().len(), 1);
    assert_eq!(
        session.document().unwrap().annotations()[0]
            .style()
            .text()
            .unwrap()
            .size_points(),
        18.0
    );

    // Finish commits any pending text before snapshotting the output job.
    session.dispatch(CaptureAction::TextBegin {
        ticket,
        point: [20.0, 12.0],
    });
    session.dispatch(CaptureAction::TextEdited {
        ticket,
        content: "Pending".to_owned(),
        bounds: selection(20.0, 12.0, 20.0, 16.0),
    });
    let job = match session
        .dispatch(CaptureAction::Finish { ticket })
        .into_iter()
        .next()
    {
        Some(CaptureEffect::ComposeOutput(job)) => job,
        _ => panic!("expected output job"),
    };
    assert_eq!(job.annotations().len(), 2);
    assert_eq!(job.version(), version + 2);

    // Output ownership disables further text edits and options.
    session.dispatch(CaptureAction::TextEdited {
        ticket,
        content: "Dropped".to_owned(),
        bounds: selection(20.0, 12.0, 20.0, 16.0),
    });
    session.dispatch(CaptureAction::TextOption {
        ticket,
        option: TextOption::Stroke(PaletteColor::Blue),
    });
    session.dispatch(CaptureAction::TextBegin {
        ticket,
        point: [20.0, 12.0],
    });
    let document = session.document().unwrap();
    assert_eq!(document.version(), version + 2);
    assert_eq!(document.annotations().len(), 2);

    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Ok(())
        }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::PlayCompletionSound
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
}

#[test]
fn core_session_29_text_cancel_discards_and_stale_tickets_never_edit() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let version = session.document().unwrap().version();

    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Text,
    });
    session.dispatch(CaptureAction::TextBegin {
        ticket,
        point: [12.0, 8.0],
    });
    session.dispatch(CaptureAction::TextEdited {
        ticket,
        content: "Discard".to_owned(),
        bounds: selection(12.0, 8.0, 20.0, 16.0),
    });
    session.dispatch(CaptureAction::TextCancel { ticket });
    assert!(session.document().unwrap().text_edit().is_none());
    assert_eq!(session.document().unwrap().version(), version);
    assert!(session.document().unwrap().annotations().is_empty());

    // A stale ticket from a dismissed overlay never edits a newer attempt.
    session.dispatch(CaptureAction::CancelAttempt(ticket));
    let CaptureEffect::CheckPermission(next) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket: next,
        available: true,
    });
    session.dispatch(CaptureAction::Completed {
        ticket: next,
        result: Ok(frame()),
    });
    session.dispatch(CaptureAction::SelectRegion {
        ticket: next,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    let new_version = session.document().unwrap().version();
    session.dispatch(CaptureAction::TextBegin {
        ticket,
        point: [12.0, 8.0],
    });
    session.dispatch(CaptureAction::TextCommit { ticket });
    let document = session.document().unwrap();
    assert_eq!(document.version(), new_version);
    assert!(document.annotations().is_empty());
    assert!(document.text_edit().is_none());
}
