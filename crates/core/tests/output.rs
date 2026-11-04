//! CORE-SESSION and APP-EFFECT: region selection, Finish, output fence and recovery.
use arcshot_core::{
    CaptureAction, CaptureEffect, CapturePhase, CaptureSession, CaptureTicket, CapturedFrame,
    OutputFailure, OutputJob, ScreenGeometry, Selection,
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

fn frozen() -> (CaptureSession, CaptureTicket) {
    let mut session = CaptureSession::default();
    let effects = session.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = effects[0] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    assert_eq!(
        session.dispatch(CaptureAction::Completed {
            ticket,
            result: Ok(frame())
        }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    (session, ticket)
}

fn select(session: &mut CaptureSession, ticket: CaptureTicket) {
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
}

fn finish(session: &mut CaptureSession, ticket: CaptureTicket) -> OutputJob {
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let CaptureEffect::ComposeOutput(job) = effects[0] else {
        panic!("expected output job");
    };
    job
}

#[test]
fn core_session_08_zero_size_drag_creates_no_selection_and_enter_does_not_finish() {
    let (mut session, ticket) = frozen();
    assert_eq!(
        session.dispatch(CaptureAction::SelectRegion {
            ticket,
            from: [10.0, 10.0],
            to: [10.0, 30.0]
        }),
        vec![]
    );
    assert!(session.document().unwrap().selection().is_none());
    assert_eq!(session.dispatch(CaptureAction::Finish { ticket }), vec![]);
    assert!(!session.output_busy());
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_09_valid_selection_then_finish_emits_one_2x_job_and_busy_mutex() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let selection = session.document().unwrap().selection().unwrap();
    assert_eq!(
        selection,
        Selection::try_new(10.0, 5.0, 30.0, 20.0).unwrap()
    );
    assert_eq!(session.document().unwrap().version(), 1);

    let job = finish(&mut session, ticket);
    assert_eq!(job.ticket(), ticket);
    assert_eq!(job.version(), 1);
    assert_eq!((job.crop().x(), job.crop().y()), (20, 10));
    assert_eq!((job.crop().width(), job.crop().height()), (60, 40));
    assert!(session.output_busy());
    assert!(session.may_publish(job));
    assert!(session.frame().is_some());
    assert_eq!(session.phase(), CapturePhase::Frozen);

    // Duplicate Finish, Start and selection edits are disabled while output runs.
    assert_eq!(session.dispatch(CaptureAction::Finish { ticket }), vec![]);
    assert_eq!(session.dispatch(CaptureAction::Start), vec![]);
    assert_eq!(
        session.dispatch(CaptureAction::SelectRegion {
            ticket,
            from: [0.0, 0.0],
            to: [5.0, 5.0]
        }),
        vec![]
    );
    assert_eq!(session.document().unwrap().selection(), Some(selection));
}

#[test]
fn app_effect_12_successful_clipboard_result_ends_session_after_fenced_publish() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let job = finish(&mut session, ticket);
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
    assert!(session.frame().is_none());
    assert!(!session.output_busy());
    assert!(!session.may_publish(job));
}

#[test]
fn app_effect_13_failed_clipboard_keeps_document_and_allows_the_same_version_retry() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let job = finish(&mut session, ticket);
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Err(OutputFailure::Clipboard)
        }),
        vec![CaptureEffect::ReportOutputFailure(OutputFailure::Clipboard)]
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(!session.output_busy());
    assert!(session.frame().is_some());
    assert!(session.document().unwrap().selection().is_some());
    assert!(!session.may_publish(job));

    let retry = finish(&mut session, ticket);
    assert_eq!(retry.version(), job.version());
    assert_eq!(retry.crop(), job.crop());
}

#[test]
fn app_effect_14_changed_document_or_cancelled_session_rejects_stale_output() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let stale = finish(&mut session, ticket);
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job: stale,
            result: Err(OutputFailure::Encode)
        }),
        vec![CaptureEffect::ReportOutputFailure(OutputFailure::Encode)]
    );

    // A newer selection bumps the version; the old job may not publish.
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [0.0, 0.0],
        to: [20.0, 20.0],
    });
    assert_eq!(session.document().unwrap().version(), 2);
    let newer = finish(&mut session, ticket);
    assert_ne!(newer, stale);
    assert!(!session.may_publish(stale));
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job: stale,
            result: Ok(())
        }),
        vec![]
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);

    // Cancelling ends the session; the late result cannot copy or reopen it.
    assert_eq!(
        session.dispatch(CaptureAction::CancelAttempt(ticket)),
        vec![CaptureEffect::DismissOverlay]
    );
    assert!(!session.may_publish(newer));
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job: newer,
            result: Ok(())
        }),
        vec![]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
}

#[test]
fn core_session_10_double_click_finishes_only_inside_the_valid_selection() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    assert_eq!(
        session.dispatch(CaptureAction::FinishAt {
            ticket,
            point: [90.0, 45.0]
        }),
        vec![]
    );
    assert!(!session.output_busy());

    let effects = session.dispatch(CaptureAction::FinishAt {
        ticket,
        point: [20.0, 10.0],
    });
    assert!(matches!(
        effects.as_slice(),
        [CaptureEffect::ComposeOutput(_)]
    ));
    assert!(session.output_busy());
}

#[test]
fn core_session_11_stale_enter_without_selection_never_composes() {
    let (mut session, ticket) = frozen();
    // An invalid drag leaves no selection; both Enter and double-click are inert.
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 10.0],
        to: [10.0, 10.0],
    });
    assert_eq!(
        session.dispatch(CaptureAction::FinishAt {
            ticket,
            point: [10.0, 10.0]
        }),
        vec![]
    );
    assert_eq!(session.dispatch(CaptureAction::Finish { ticket }), vec![]);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_12_cancel_during_output_prevents_the_late_clipboard_side_effect() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let job = finish(&mut session, ticket);
    assert!(session.may_publish(job));
    assert_eq!(
        session.dispatch(CaptureAction::Cancel),
        vec![CaptureEffect::DismissOverlay]
    );
    assert!(!session.may_publish(job));
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Ok(())
        }),
        vec![]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
}

#[test]
fn core_session_13_output_busy_ignores_a_new_start_even_from_the_hotkey() {
    let (mut session, ticket) = frozen();
    select(&mut session, ticket);
    let _job = finish(&mut session, ticket);
    assert!(session.output_busy());
    assert_eq!(session.dispatch(CaptureAction::Start), vec![]);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}
