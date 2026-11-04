//! CORE-SESSION: selection drags, reselect, undo/redo and new-session history.
use arcshot_core::{
    CaptureAction, CaptureEffect, CapturePhase, CaptureSession, CaptureTicket, CapturedFrame,
    OutputFailure, PaletteColor, RectangleOption, ScreenGeometry, Selection, Tool,
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
fn core_session_14_a_selection_drag_is_live_then_one_undoable_step() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let before = session.document().unwrap().version();
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(10.0, 5.0, 30.0, 20.0))
    );

    assert_eq!(
        session.dispatch(CaptureAction::PointerPressed {
            ticket,
            point: [20.0, 10.0]
        }),
        vec![]
    );
    assert!(session.document().unwrap().dragging());
    session.dispatch(CaptureAction::PointerMoved {
        ticket,
        point: [24.0, 13.0],
    });
    // The overlay preview follows the authoritative document during the drag.
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(14.0, 8.0, 30.0, 20.0))
    );
    assert_eq!(session.document().unwrap().version(), before);

    session.dispatch(CaptureAction::PointerReleased { ticket });
    assert!(!session.document().unwrap().dragging());
    assert_eq!(session.document().unwrap().version(), before + 1);

    assert_eq!(session.dispatch(CaptureAction::Undo { ticket }), vec![]);
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(10.0, 5.0, 30.0, 20.0))
    );
    assert_eq!(session.dispatch(CaptureAction::Redo { ticket }), vec![]);
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(14.0, 8.0, 30.0, 20.0))
    );
}

#[test]
fn core_session_15_reselect_replaces_the_document_as_one_undoable_step() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let version = session.document().unwrap().version();

    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [50.0, 20.0],
        to: [70.0, 30.0],
    });
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(50.0, 20.0, 20.0, 10.0))
    );
    assert_eq!(session.document().unwrap().version(), version + 1);

    session.dispatch(CaptureAction::Undo { ticket });
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(10.0, 5.0, 30.0, 20.0))
    );
    session.dispatch(CaptureAction::Redo { ticket });
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(50.0, 20.0, 20.0, 10.0))
    );
}

#[test]
fn core_session_16_a_new_capture_starts_with_empty_history() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    session.dispatch(CaptureAction::PointerPressed {
        ticket,
        point: [20.0, 10.0],
    });
    session.dispatch(CaptureAction::PointerMoved {
        ticket,
        point: [24.0, 13.0],
    });
    session.dispatch(CaptureAction::PointerReleased { ticket });

    // Restarting freezes a new base; the old document and its history are gone.
    let effects = session.dispatch(CaptureAction::Start);
    assert!(matches!(
        effects.as_slice(),
        [
            CaptureEffect::DismissOverlay,
            CaptureEffect::CheckPermission(_)
        ]
    ));
    let CaptureEffect::CheckPermission(next) = effects[1] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket: next,
        available: true,
    });
    assert_eq!(
        session.dispatch(CaptureAction::Completed {
            ticket: next,
            result: Ok(frame())
        }),
        vec![CaptureEffect::PresentFrozen(next)]
    );
    {
        let document = session.document().unwrap();
        assert_eq!(document.selection(), None);
        assert_eq!(document.version(), 0);
    }
    assert_eq!(
        session.dispatch(CaptureAction::Undo { ticket: next }),
        vec![]
    );
    assert_eq!(session.document().unwrap().selection(), None);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_17_output_mutex_ignores_edits_and_history_until_the_result_is_in() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let job = match session
        .dispatch(CaptureAction::Finish { ticket })
        .into_iter()
        .next()
    {
        Some(CaptureEffect::ComposeOutput(job)) => job,
        _ => panic!("expected output job"),
    };

    assert_eq!(session.dispatch(CaptureAction::Undo { ticket }), vec![]);
    assert_eq!(session.dispatch(CaptureAction::Redo { ticket }), vec![]);
    assert_eq!(
        session.dispatch(CaptureAction::PointerPressed {
            ticket,
            point: [20.0, 10.0]
        }),
        vec![]
    );
    assert_eq!(
        session.document().unwrap().selection(),
        Some(selection(10.0, 5.0, 30.0, 20.0))
    );
    assert!(!session.document().unwrap().dragging());

    // A failed clipboard releases the mutex; editing resumes and the old job is fenced.
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job: job.clone(),
            result: Err(OutputFailure::Clipboard)
        }),
        vec![CaptureEffect::ReportOutputFailure(OutputFailure::Clipboard)]
    );
    assert_eq!(session.dispatch(CaptureAction::Undo { ticket }), vec![]);
    assert_eq!(session.document().unwrap().selection(), None);
    assert!(!session.may_publish(&job));
}

#[test]
fn core_session_18_stale_ticket_edits_and_history_never_touch_a_newer_attempt() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    session.dispatch(CaptureAction::CancelAttempt(ticket));
    assert_eq!(session.phase(), CapturePhase::Resident);

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

    assert_eq!(
        session.dispatch(CaptureAction::PointerPressed {
            ticket,
            point: [20.0, 10.0]
        }),
        vec![]
    );
    assert_eq!(session.dispatch(CaptureAction::Undo { ticket }), vec![]);
    assert_eq!(session.dispatch(CaptureAction::Redo { ticket }), vec![]);
    let document = session.document().unwrap();
    assert_eq!(document.selection(), Some(selection(10.0, 5.0, 30.0, 20.0)));
    assert!(!document.dragging());
}

#[test]
fn core_session_19_undo_without_history_is_a_no_op() {
    let (mut session, ticket) = frozen();
    let before = session.document().unwrap().version();
    assert_eq!(session.dispatch(CaptureAction::Undo { ticket }), vec![]);
    assert_eq!(session.dispatch(CaptureAction::Redo { ticket }), vec![]);
    let document = session.document().unwrap();
    assert_eq!(document.version(), before);
    assert_eq!(document.selection(), None);
}

#[test]
fn core_session_24_annotation_actions_share_the_edit_fence_and_history() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);

    // The tool is session state; switching it does not enter history.
    let version = session.document().unwrap().version();
    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Rectangle,
    });
    assert_eq!(session.document().unwrap().tool(), Tool::Rectangle);
    assert_eq!(session.document().unwrap().version(), version);

    // One pointer gesture is one live preview followed by exactly one step.
    session.dispatch(CaptureAction::PointerPressed {
        ticket,
        point: [12.0, 8.0],
    });
    session.dispatch(CaptureAction::PointerMoved {
        ticket,
        point: [18.0, 14.0],
    });
    assert_eq!(session.document().unwrap().annotations().len(), 1);
    assert_eq!(session.document().unwrap().version(), version);
    session.dispatch(CaptureAction::PointerReleased { ticket });
    let document = session.document().unwrap();
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(document.selected_index(), Some(0));
    assert_eq!(document.version(), version + 1);

    // A style option edits the selected object and is undoable.
    session.dispatch(CaptureAction::RectangleOption {
        ticket,
        option: RectangleOption::Stroke(PaletteColor::Blue),
    });
    assert_eq!(
        session.document().unwrap().annotations()[0]
            .style()
            .stroke(),
        PaletteColor::Blue
    );
    assert_eq!(session.document().unwrap().version(), version + 2);

    // Delete and undo run through the same document.
    session.dispatch(CaptureAction::DeleteSelected { ticket });
    assert!(session.document().unwrap().annotations().is_empty());
    assert_eq!(session.document().unwrap().version(), version + 3);
    session.dispatch(CaptureAction::Undo { ticket });
    assert_eq!(session.document().unwrap().annotations().len(), 1);
    session.dispatch(CaptureAction::Redo { ticket });
    assert!(session.document().unwrap().annotations().is_empty());

    // Output ownership disables tool, option and delete edits.
    let job = match session
        .dispatch(CaptureAction::Finish { ticket })
        .into_iter()
        .next()
    {
        Some(CaptureEffect::ComposeOutput(job)) => job,
        _ => panic!("expected output job"),
    };
    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Rectangle,
    });
    session.dispatch(CaptureAction::RectangleOption {
        ticket,
        option: RectangleOption::Fill(true),
    });
    session.dispatch(CaptureAction::DeleteSelected { ticket });
    session.dispatch(CaptureAction::DeselectSelected { ticket });
    let document = session.document().unwrap();
    assert_eq!(document.tool(), Tool::Rectangle);
    assert!(!document.rectangle_style().fill());
    assert_eq!(document.version(), version + 5);
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
}
