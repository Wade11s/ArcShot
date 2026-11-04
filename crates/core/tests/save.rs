//! CORE-SESSION and APP-EFFECT: explicit Save PNG, cancel, failure and output fences.
use arcshot_core::{
    CaptureAction, CaptureEffect, CapturePhase, CaptureSession, CaptureTicket, CapturedFrame,
    OutputFailure, OutputJob, OutputTarget, SavePanelResult, ScreenGeometry, Selection,
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
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    session.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    (session, ticket)
}

/// A frozen document with the 2x selection (10,5)-(40,25) at version 1.
fn editing() -> (CaptureSession, CaptureTicket) {
    let (mut session, ticket) = frozen();
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    assert_eq!(session.document().unwrap().version(), 1);
    (session, ticket)
}

fn request_save(session: &mut CaptureSession, ticket: CaptureTicket) -> OutputJob {
    let effects = session.dispatch(CaptureAction::Save { ticket });
    let CaptureEffect::RequestSavePanel(job) = effects[0] else {
        panic!("expected a native save panel request");
    };
    job
}

fn exchange(
    session: &mut CaptureSession,
    job: OutputJob,
    result: SavePanelResult,
) -> Vec<CaptureEffect> {
    session.dispatch(CaptureAction::SavePanelResolved { job, result })
}

fn finish(session: &mut CaptureSession, ticket: CaptureTicket) -> OutputJob {
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let CaptureEffect::ComposeOutput(job) = effects[0] else {
        panic!("expected the clipboard output job");
    };
    job
}

#[test]
fn core_session_20_open_save_panel_blocks_editing_finish_save_and_hotkey_start() {
    let (mut session, ticket) = editing();
    let job = request_save(&mut session, ticket);
    assert_eq!(job.target(), OutputTarget::SaveFile);
    assert_eq!(job.version(), 1);
    assert_eq!((job.crop().x(), job.crop().y()), (20, 10));
    assert_eq!((job.crop().width(), job.crop().height()), (60, 40));
    assert!(session.output_busy());
    assert_eq!(session.phase(), CapturePhase::Frozen);
    // The panel phase has no encoded bytes yet, so there is nothing to publish.
    assert!(!session.may_publish(job));

    let selection = session.document().unwrap().selection();
    assert_eq!(session.dispatch(CaptureAction::Finish { ticket }), vec![]);
    assert_eq!(session.dispatch(CaptureAction::Save { ticket }), vec![]);
    assert_eq!(
        session.dispatch(CaptureAction::FinishAt {
            ticket,
            point: [20.0, 10.0]
        }),
        vec![]
    );
    assert_eq!(
        session.dispatch(CaptureAction::SelectRegion {
            ticket,
            from: [0.0, 0.0],
            to: [5.0, 5.0]
        }),
        vec![]
    );
    assert_eq!(session.dispatch(CaptureAction::Start), vec![]);
    assert_eq!(session.document().unwrap().selection(), selection);
    assert_eq!(session.document().unwrap().version(), 1);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_21_cancelled_save_panel_returns_to_the_untouched_session_without_error() {
    let (mut session, ticket) = editing();
    let job = request_save(&mut session, ticket);
    assert_eq!(
        exchange(&mut session, job, SavePanelResult::Cancelled),
        vec![]
    );
    assert!(!session.output_busy());
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(!session.may_publish(job));
    let document = session.document().unwrap();
    assert_eq!(document.version(), 1);
    assert_eq!(
        document.selection(),
        Some(Selection::try_new(10.0, 5.0, 30.0, 20.0).unwrap())
    );

    // The closed panel cannot resolve twice, and a retry opens the same snapshot.
    assert_eq!(
        exchange(&mut session, job, SavePanelResult::Cancelled),
        vec![]
    );
    assert_eq!(request_save(&mut session, ticket), job);
}

#[test]
fn core_session_22_failed_save_panel_reports_a_classified_error_and_keeps_the_document() {
    let (mut session, ticket) = editing();
    let job = request_save(&mut session, ticket);
    assert_eq!(
        exchange(&mut session, job, SavePanelResult::Failed),
        vec![CaptureEffect::ReportOutputFailure(OutputFailure::SavePanel)]
    );
    assert!(!session.output_busy());
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(session.document().unwrap().selection().is_some());
    assert_eq!(session.document().unwrap().version(), job.version());

    let retry = request_save(&mut session, ticket);
    assert_eq!(retry.version(), job.version());
    assert_eq!(retry.crop(), job.crop());
}

#[test]
fn core_session_23_save_requires_a_frozen_selection_and_the_current_ticket() {
    let (mut session, ticket) = frozen();
    assert_eq!(session.dispatch(CaptureAction::Save { ticket }), vec![]);
    assert!(!session.output_busy());

    let (mut session, ticket) = editing();
    session.dispatch(CaptureAction::Cancel);
    assert_eq!(session.dispatch(CaptureAction::Save { ticket }), vec![]);
    assert!(session.document().is_none());
}

#[test]
fn app_effect_18_chosen_save_composes_one_save_job_and_ends_without_a_clipboard_step() {
    let (mut session, ticket) = editing();
    let job = request_save(&mut session, ticket);
    assert_eq!(
        exchange(&mut session, job, SavePanelResult::Chosen),
        vec![CaptureEffect::ComposeOutput(job)]
    );
    assert!(session.output_busy());
    assert!(session.may_publish(job));
    assert_eq!(session.dispatch(CaptureAction::Start), vec![]);

    // Only the confirmed file write reports success; the session then ends.
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
}

#[test]
fn app_effect_19_classified_save_failures_keep_the_document_for_the_same_version_retry() {
    for failure in [
        OutputFailure::SaveFile,
        OutputFailure::ResourceLimit,
        OutputFailure::Encode,
    ] {
        let (mut session, ticket) = editing();
        let job = request_save(&mut session, ticket);
        exchange(&mut session, job, SavePanelResult::Chosen);
        assert_eq!(
            session.dispatch(CaptureAction::OutputCompleted {
                job,
                result: Err(failure)
            }),
            vec![CaptureEffect::ReportOutputFailure(failure)]
        );
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert!(!session.output_busy());
        assert!(session.frame().is_some());
        assert!(session.document().unwrap().selection().is_some());
        assert!(!session.may_publish(job));

        let retry = request_save(&mut session, ticket);
        assert_eq!(retry.version(), job.version());
        assert_eq!(retry.crop(), job.crop());
    }
}

#[test]
fn app_effect_20_stale_save_panel_results_and_late_writes_never_publish() {
    // A result that arrives after the session was cancelled is discarded.
    let (mut session, ticket) = editing();
    let job = request_save(&mut session, ticket);
    assert_eq!(
        session.dispatch(CaptureAction::CancelAttempt(ticket)),
        vec![CaptureEffect::DismissOverlay]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert_eq!(exchange(&mut session, job, SavePanelResult::Chosen), vec![]);
    assert!(!session.may_publish(job));
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Ok(())
        }),
        vec![]
    );

    // A panel result can never resolve a clipboard finish job.
    let (mut session, ticket) = editing();
    let clipboard = finish(&mut session, ticket);
    assert_eq!(clipboard.target(), OutputTarget::Clipboard);
    assert_eq!(
        exchange(&mut session, clipboard, SavePanelResult::Chosen),
        vec![]
    );
    assert!(session.output_busy());

    // The same closed panel cannot compose twice while the write is pending.
    let (mut session, ticket) = editing();
    let job = request_save(&mut session, ticket);
    assert_eq!(
        exchange(&mut session, job, SavePanelResult::Chosen),
        vec![CaptureEffect::ComposeOutput(job)]
    );
    assert_eq!(exchange(&mut session, job, SavePanelResult::Chosen), vec![]);
}

#[test]
fn app_effect_21_finish_and_save_share_one_snapshot_but_never_start_each_other() {
    let (mut session, ticket) = editing();
    let clipboard = finish(&mut session, ticket);
    assert_eq!(clipboard.target(), OutputTarget::Clipboard);
    // While the clipboard job runs, Save is ignored rather than queued.
    assert_eq!(session.dispatch(CaptureAction::Save { ticket }), vec![]);
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job: clipboard,
            result: Err(OutputFailure::Clipboard)
        }),
        vec![CaptureEffect::ReportOutputFailure(OutputFailure::Clipboard)]
    );

    // A failed clipboard output leaves the same snapshot for the explicit save.
    let save = request_save(&mut session, ticket);
    assert_eq!(save.ticket(), clipboard.ticket());
    assert_eq!(save.version(), clipboard.version());
    assert_eq!(save.crop(), clipboard.crop());
    assert_ne!(save.target(), clipboard.target());
    assert_ne!(save, clipboard);
    assert!(!session.may_publish(clipboard));
    assert_eq!(
        exchange(&mut session, clipboard, SavePanelResult::Chosen),
        vec![]
    );
    assert!(session.output_busy());
}
