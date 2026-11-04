//! CORE-SESSION: the protected single-frame Scroll collection loop (I-11 3/6/10, I-12).
//!
//! Every case drives the authoritative session from a frozen selection through
//! preparation into collection, then observes completion, cancel, restart and
//! failure handling without touching native code.
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
    CapturedFrame, PixelRect, ScreenGeometry, ScrollFrameError, ScrollInputError, ScrollPhase,
    ScrollRefusal, ScrollTarget, ScrollTargetError, ScrollTargetFrame, ScrollWindowError,
    Selection, Tool, scroll_frame_geometry,
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

/// A frozen session with the standard (10,5) 30x20 selection.
fn frozen_selected() -> (CaptureSession, CaptureTicket) {
    let mut session = CaptureSession::default();
    let effects = session.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = effects[0] else {
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
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    (session, ticket)
}

fn permissions_ok(session: &mut CaptureSession, ticket: CaptureTicket) {
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: arcshot_core::ScrollPermissions {
            screen_recording: true,
            input_monitoring: true,
            accessibility: true,
        },
    });
}

fn target() -> ScrollTarget {
    ScrollTarget {
        window_id: 4242,
        owner: "com.example.target".to_owned(),
        frame: ScrollTargetFrame {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 40.0,
        },
    }
}

/// Drives preparation to `ScrollPhase::Prepared` with the standard selection.
fn prepared() -> (CaptureSession, CaptureTicket) {
    let (mut session, ticket) = frozen_selected();
    session.dispatch(CaptureAction::EnterScroll { ticket });
    permissions_ok(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollTargetIdentified {
        ticket,
        result: Ok(target()),
    });
    session.dispatch(CaptureAction::ScrollWindowsPrepared {
        ticket,
        result: Ok(vec![11, 12]),
    });
    assert_eq!(session.scroll_phase(), ScrollPhase::Prepared);
    (session, ticket)
}

/// The physical crop of the standard selection on the standard screen.
fn selection_crop(session: &CaptureSession) -> PixelRect {
    let document = session.document().unwrap();
    document
        .selection()
        .unwrap()
        .pixel_rect(document.frame().geometry())
        .unwrap()
}

/// A trusted initial frame that exactly covers the fixed selection.
fn scroll_frame(session: &CaptureSession) -> CapturedFrame {
    let document = session.document().unwrap();
    let crop = document
        .selection()
        .unwrap()
        .pixel_rect(document.frame().geometry())
        .unwrap();
    let geometry = scroll_frame_geometry(document.frame().geometry(), crop).unwrap();
    let [w, h] = geometry.pixel_size();
    CapturedFrame::try_from_srgb_bgra(
        geometry,
        w,
        h,
        w as usize * 4,
        vec![255; w as usize * h as usize * 4],
    )
    .unwrap()
}

/// Drives preparation to `ScrollPhase::Collecting` with a trusted first frame.
fn collecting() -> (CaptureSession, CaptureTicket) {
    let (mut session, ticket) = prepared();
    session.dispatch(CaptureAction::ScrollInputInstalled {
        ticket,
        result: Ok(()),
    });
    session.dispatch(CaptureAction::ScrollFrameAdmitted {
        ticket,
        result: Ok(scroll_frame(&session)),
    });
    assert_eq!(session.scroll_phase(), ScrollPhase::Collecting);
    (session, ticket)
}

/// The one output job a single-frame completion submits.
fn completion_job(session: &mut CaptureSession, ticket: CaptureTicket) -> CaptureEffect {
    let effects = session.dispatch(CaptureAction::CompleteScroll { ticket });
    effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("single-frame completion submits one compose job")
}

#[test]
fn core_session_46_installed_input_requests_the_trusted_first_frame() {
    let (mut session, ticket) = prepared();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollInputInstalled {
            ticket,
            result: Ok(())
        }),
        vec![CaptureEffect::CaptureScrollFrame(ticket)]
    );
    // The collection handshake is still closed: no scroll pass-through yet.
    assert_eq!(session.scroll_phase(), ScrollPhase::Prepared);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_47_input_install_failures_refuse_and_keep_the_region_session() {
    for error in [
        ScrollInputError::NotFrontmost,
        ScrollInputError::TapUnavailable,
        ScrollInputError::GestureInProgress,
        ScrollInputError::StillDraining,
    ] {
        let (mut session, ticket) = prepared();
        let selection = session.document().unwrap().selection();
        assert_eq!(
            session.dispatch(CaptureAction::ScrollInputInstalled {
                ticket,
                result: Err(error)
            }),
            vec![
                CaptureEffect::ScrollRefused(ticket, ScrollRefusal::Input(error)),
                CaptureEffect::PresentFrozen(ticket)
            ]
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert_eq!(session.document().unwrap().selection(), selection);
        assert!(!session.output_busy());
    }
}

#[test]
fn core_session_48_trusted_first_frame_opens_collection() {
    let (mut session, ticket) = prepared();
    session.dispatch(CaptureAction::ScrollInputInstalled {
        ticket,
        result: Ok(()),
    });
    let frame = scroll_frame(&session);
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameAdmitted {
            ticket,
            result: Ok(frame)
        }),
        vec![CaptureEffect::ScrollCollecting(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Collecting);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    // The region document and its selection are untouched by collection.
    assert_eq!(
        selection_crop(&session),
        PixelRect::try_new(20, 10, 60, 40).unwrap()
    );
}

#[test]
fn core_session_49_frame_failures_classify_into_refusal_terminal_or_session_cancel() {
    // Known-state failures refuse without output and keep the region session.
    for (failure, expected) in [
        (CaptureFailure::InvalidFrame, ScrollFrameError::Admission),
        (
            CaptureFailure::ResourceLimit,
            ScrollFrameError::ResourceLimit,
        ),
        (CaptureFailure::WorkerUnavailable, ScrollFrameError::Worker),
    ] {
        let (mut session, ticket) = prepared();
        session.dispatch(CaptureAction::ScrollInputInstalled {
            ticket,
            result: Ok(()),
        });
        let selection = session.document().unwrap().selection();
        assert_eq!(
            session.dispatch(CaptureAction::ScrollFrameAdmitted {
                ticket,
                result: Err(failure)
            }),
            vec![
                CaptureEffect::ScrollRefused(ticket, ScrollRefusal::Frame(expected)),
                CaptureEffect::PresentFrozen(ticket)
            ]
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert_eq!(session.document().unwrap().selection(), selection);
    }

    // Uncertain native state is terminal: no output, no in-session restart.
    for failure in [
        CaptureFailure::NativeUncertain,
        CaptureFailure::WorkerPanicked,
    ] {
        let (mut session, ticket) = prepared();
        session.dispatch(CaptureAction::ScrollInputInstalled {
            ticket,
            result: Ok(()),
        });
        assert_eq!(
            session.dispatch(CaptureAction::ScrollFrameAdmitted {
                ticket,
                result: Err(failure)
            }),
            vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ScrollRelaunchRequired
            ]
        );
        assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
        assert_eq!(
            session.dispatch(CaptureAction::Start),
            vec![CaptureEffect::ReportFailure(
                CaptureFailure::NativeUncertain
            )]
        );
    }

    // Display loss cancels the whole session; no partial output, no overlay.
    let (mut session, ticket) = prepared();
    session.dispatch(CaptureAction::ScrollInputInstalled {
        ticket,
        result: Ok(()),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameAdmitted {
            ticket,
            result: Err(CaptureFailure::DisplayInvalid)
        }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid)
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(session.frame().is_none());
}

#[test]
fn core_session_50_admitted_frame_must_match_the_fixed_selection_geometry() {
    let (mut session, ticket) = prepared();
    session.dispatch(CaptureAction::ScrollInputInstalled {
        ticket,
        result: Ok(()),
    });
    // A well-formed frame of the wrong size (e.g. the full screen) is refused.
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameAdmitted {
            ticket,
            result: Ok(frame())
        }),
        vec![
            CaptureEffect::ScrollRefused(ticket, ScrollRefusal::Frame(ScrollFrameError::Geometry)),
            CaptureEffect::PresentFrozen(ticket)
        ]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_51_enter_completes_the_single_frame_without_any_scrolling() {
    let (mut session, ticket) = collecting();
    let job = completion_job(&mut session, ticket);
    let CaptureEffect::ComposeOutput(job) = job else {
        panic!("expected one compose job");
    };
    assert_eq!(job.ticket(), ticket);
    assert_eq!(job.target(), arcshot_core::OutputTarget::Clipboard);
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
    assert!(job.annotations().is_empty());
    // The completion document is the scroll frame; publishing is fenced.
    assert!(session.may_publish(&job));
    assert_eq!(
        session.document().unwrap().selection().unwrap().width(),
        30.0
    );
    // Repeated completion and the capture hotkey are blocked while output runs.
    assert_eq!(
        session.dispatch(CaptureAction::CompleteScroll { ticket }),
        vec![]
    );
    assert_eq!(session.dispatch(CaptureAction::Start), vec![]);
    assert!(session.output_busy());
}

#[test]
fn core_session_52_successful_completion_ends_the_session_and_respects_the_sound_preference() {
    // Sound on (default): the session ends and one sound effect is requested.
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
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
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(session.frame().is_none());
    assert!(!session.output_busy());

    // Sound off: success stays silent; the failure contract is unchanged.
    let (mut session, ticket) = collecting();
    session.set_completion_sound(false);
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Ok(())
        }),
        vec![CaptureEffect::DismissOverlay]
    );
}

#[test]
fn core_session_53_failed_completion_keeps_collection_and_allows_retry() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Err(arcshot_core::OutputFailure::Clipboard)
        }),
        vec![CaptureEffect::ReportOutputFailure(
            arcshot_core::OutputFailure::Clipboard
        )]
    );
    // The collected result survives: collection continues and Enter retries.
    assert_eq!(session.scroll_phase(), ScrollPhase::Collecting);
    assert!(!session.output_busy());
    assert_eq!(
        selection_crop(&session),
        PixelRect::try_new(20, 10, 60, 40).unwrap()
    );
    let CaptureEffect::ComposeOutput(_) = completion_job(&mut session, ticket) else {
        panic!("retry submits a fresh compose job");
    };
}

#[test]
fn core_session_54_esc_abandons_the_scroll_result_and_returns_the_pre_scroll_selection() {
    let (mut session, ticket) = collecting();
    let selection = session.document().unwrap().selection().unwrap();
    let version = session.document().unwrap().version();
    assert_eq!(
        session.dispatch(CaptureAction::CancelScroll { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    // The same frozen document returns: no re-capture, no version change.
    assert_eq!(session.document().unwrap().selection(), Some(selection));
    assert_eq!(session.document().unwrap().version(), version);
    // The abandoned frame can no longer complete anything.
    assert_eq!(
        session.dispatch(CaptureAction::CompleteScroll { ticket }),
        vec![]
    );
}

#[test]
fn core_session_55_esc_while_output_runs_cancels_the_publish_fence() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(
        session.dispatch(CaptureAction::CancelScroll { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert!(!session.output_busy());
    assert!(!session.may_publish(&job));
    // The late clipboard result publishes nothing and plays no sound.
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Ok(())
        }),
        vec![]
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection().unwrap().x(), 10.0);
}

#[test]
fn core_session_56_hotkey_restart_abandons_collection_but_waits_for_output() {
    let (mut session, _ticket) = collecting();
    let effects = session.dispatch(CaptureAction::Start);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CaptureEffect::CheckPermission(_)))
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::CheckingPermission);

    // While a scroll output is in flight the hotkey stays blocked.
    let (mut session, ticket) = collecting();
    completion_job(&mut session, ticket);
    assert_eq!(session.dispatch(CaptureAction::Start), vec![]);
    assert_eq!(session.scroll_phase(), ScrollPhase::Collecting);
}

#[test]
fn core_session_57_input_protection_loss_stops_collection_without_output() {
    let (mut session, ticket) = collecting();
    let selection = session.document().unwrap().selection();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollInputLost { ticket }),
        vec![
            CaptureEffect::ScrollRefused(
                ticket,
                ScrollRefusal::Input(ScrollInputError::ProtectionLost)
            ),
            CaptureEffect::PresentFrozen(ticket)
        ]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection(), selection);
    assert!(!session.output_busy());
}

#[test]
fn core_session_58_target_loss_during_collection_stops_without_output() {
    for error in [
        ScrollTargetError::NoTarget,
        ScrollTargetError::NotFrontmost,
        ScrollTargetError::Geometry,
    ] {
        let (mut session, ticket) = collecting();
        let selection = session.document().unwrap().selection();
        assert_eq!(
            session.dispatch(CaptureAction::ScrollTargetLost { ticket, error }),
            vec![
                CaptureEffect::ScrollRefused(ticket, ScrollRefusal::Target(error)),
                CaptureEffect::PresentFrozen(ticket)
            ]
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert_eq!(session.document().unwrap().selection(), selection);
    }
}

#[test]
fn core_session_59_stale_scroll_actions_never_affect_another_attempt() {
    let (mut session, ticket) = prepared();
    session.dispatch(CaptureAction::CancelScroll { ticket });
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    // Late native results from the cancelled preparation change nothing.
    assert_eq!(
        session.dispatch(CaptureAction::ScrollInputInstalled {
            ticket,
            result: Ok(())
        }),
        vec![]
    );
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameAdmitted {
            ticket,
            result: Ok(scroll_frame(&session))
        }),
        vec![]
    );
    assert_eq!(
        session.dispatch(CaptureAction::CompleteScroll { ticket }),
        vec![]
    );
    assert_eq!(
        session.dispatch(CaptureAction::ScrollInputLost { ticket }),
        vec![]
    );
    assert_eq!(
        session.dispatch(CaptureAction::ScrollTargetLost {
            ticket,
            error: ScrollTargetError::NoTarget
        }),
        vec![]
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);
    // The user may enter Scroll again with a new preparation on the same ticket.
    session.dispatch(CaptureAction::EnterScroll { ticket });
    assert_eq!(session.scroll_phase(), ScrollPhase::CheckingPermissions);
}

#[test]
fn core_session_60_duplicate_frame_admission_is_ignored_not_queued() {
    let (mut session, ticket) = collecting();
    // The single-frame loop never queues a second raw frame.
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameAdmitted {
            ticket,
            result: Ok(scroll_frame(&session))
        }),
        vec![]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Collecting);
    // The original completion still works exactly once.
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
}

// CORE-COORD-10: the display+crop → frame-geometry conversion is single and budget-fenced.
#[test]
fn core_coord_10_scroll_geometry_is_the_single_selection_conversion() {
    // scroll_frame_geometry is the one display+crop → frame-geometry conversion.
    let geometry =
        scroll_frame_geometry(screen(), PixelRect::try_new(20, 10, 60, 40).unwrap()).unwrap();
    assert_eq!(geometry.display_id(), 1);
    assert_eq!(geometry.screen_origin(), [10.0, 5.0]);
    assert_eq!(geometry.pixel_size(), [60, 40]);
    assert_eq!(geometry.scale(), 2.0);
    // The largest crop PixelRect admits still converts; PixelRect and
    // ScreenGeometry enforce the 16,384 edge and 24M pixel budgets on the way
    // in, so an over-budget crop can never reach this conversion.
    let wide = PixelRect::try_new(0, 0, 16_384, 1_464).unwrap();
    assert_eq!(wide.pixel_count(), 23_986_176);
    assert!(scroll_frame_geometry(screen(), wide).is_ok());
}

/// Guard: the fixtures above still build a real region session with one
/// annotation-free selection, so every loop case starts from honest state.
#[test]
fn core_session_fixtures_still_use_valid_region_sessions() {
    let (session, _ticket) = frozen_selected();
    assert_eq!(session.phase(), CapturePhase::Frozen);
    let document = session.document().unwrap();
    assert!(document.selection().is_some());
    assert!(document.annotations().is_empty());
    assert_eq!(document.tool(), Tool::Select);
    assert!(session.scroll_selection().is_none());
    assert_eq!(
        ScrollRefusal::Windows(ScrollWindowError::MappingFailed),
        ScrollRefusal::Windows(ScrollWindowError::MappingFailed)
    );
    assert!(Selection::try_new(0.0, 0.0, 1.0, 1.0).is_ok());
}
