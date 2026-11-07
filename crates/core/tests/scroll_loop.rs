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
        owner_pid: 999,
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
fn core_session_52_successful_completion_shows_the_preview_and_keeps_the_shared_document() {
    // Sound on (default): the whole-image document stays authoritative so the
    // user can annotate it, and one sound effect is requested.
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
            CaptureEffect::ShowCompletionPreview(ticket),
            CaptureEffect::PlayCompletionSound
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    // The completion document is the shared long image, not the region crop.
    let document = session.document().unwrap();
    assert_eq!(document.selection().unwrap().width(), 30.0);
    assert_eq!(document.selection().unwrap().height(), 20.0);
    assert!(session.completion_preview().is_some());
    assert!(!session.output_busy());

    // Sound off: the preview still appears; only the success sound changes.
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
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ShowCompletionPreview(ticket)
        ]
    );
}

#[test]
fn core_session_53_failed_completion_copy_shows_no_preview_and_allows_retry() {
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
    // No preview may be requested by a copy failure, and the retry is fresh.
    assert!(session.completion_preview().is_none());
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
    // #34: a known stop with verified history offers the explicit choice
    // instead of outputting or returning straight to the overlay.
    assert_eq!(
        session.dispatch(CaptureAction::ScrollInputLost { ticket }),
        vec![CaptureEffect::ScrollPartialChoice(
            ticket,
            ScrollRefusal::Input(ScrollInputError::ProtectionLost)
        )]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
    assert!(session.scroll_partial_available());
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
            vec![CaptureEffect::ScrollPartialChoice(
                ticket,
                ScrollRefusal::Target(error)
            )]
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
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

// CORE-SESSION: multi-frame verification, reverse de-duplication and the
// verified-stitch completion document (I-11 §4).

/// A verified stitched document of an arbitrary physical height.
fn stitched_frame(height: u32) -> CapturedFrame {
    let [width, _] = screen().pixel_size();
    let geometry = ScreenGeometry::try_new(
        1,
        0.0,
        0.0,
        f64::from(width) / 2.0,
        f64::from(height) / 2.0,
        2.0,
    )
    .unwrap();
    CapturedFrame::try_from_srgb_bgra(
        geometry,
        width,
        height,
        width as usize * 4,
        vec![255; width as usize * height as usize * 4],
    )
    .unwrap()
}

#[test]
fn core_session_61_collected_frame_is_held_for_verification() {
    let (mut session, ticket) = collecting();
    let frame = scroll_frame(&session);
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameCollected {
            ticket,
            result: Ok(frame)
        }),
        vec![CaptureEffect::VerifyScrollFrame(ticket)]
    );
    // Verification is required before history or completion may change.
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
}

#[test]
fn core_session_62_verified_extension_replaces_the_completion_document() {
    let (mut session, ticket) = collecting();
    session.dispatch(CaptureAction::ScrollFrameCollected {
        ticket,
        result: Ok(scroll_frame(&session)),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollStitchVerified {
            ticket,
            frame: stitched_frame(80),
            extended: true,
        }),
        vec![CaptureEffect::ScrollPreviewUpdated(ticket)]
    );
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 200, 80).unwrap());
}

#[test]
fn core_session_63_reverse_pass_does_not_change_the_completion_document() {
    let (mut session, ticket) = collecting();
    session.dispatch(CaptureAction::ScrollFrameCollected {
        ticket,
        result: Ok(scroll_frame(&session)),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollStitchVerified {
            ticket,
            frame: scroll_frame(&session),
            extended: false,
        }),
        vec![CaptureEffect::ScrollPreviewUpdated(ticket)]
    );
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
}

#[test]
fn core_session_64_stitch_refusal_stops_collection_without_output() {
    let (mut session, ticket) = collecting();
    let selection = session.document().unwrap().selection();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollStitchRefused {
            ticket,
            error: arcshot_core::ScrollStitchError::Gap,
        }),
        vec![CaptureEffect::ScrollPartialChoice(
            ticket,
            ScrollRefusal::Stitch(arcshot_core::ScrollStitchError::Gap)
        )]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection(), selection);
    assert!(!session.output_busy());
    // Normal completion is no longer reachable after a verification refusal;
    // only the explicit partial accept may publish.
    assert_eq!(
        session.dispatch(CaptureAction::CompleteScroll { ticket }),
        vec![]
    );
}

#[test]
fn core_session_65_collected_capture_failures_classify_like_the_initial_frame() {
    for (failure, expected) in [
        (CaptureFailure::InvalidFrame, ScrollFrameError::Admission),
        (
            CaptureFailure::ResourceLimit,
            ScrollFrameError::ResourceLimit,
        ),
        (CaptureFailure::WorkerUnavailable, ScrollFrameError::Worker),
    ] {
        let (mut session, ticket) = collecting();
        let selection = session.document().unwrap().selection();
        assert_eq!(
            session.dispatch(CaptureAction::ScrollFrameCollected {
                ticket,
                result: Err(failure),
            }),
            vec![CaptureEffect::ScrollPartialChoice(
                ticket,
                ScrollRefusal::Frame(expected)
            )]
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
        assert_eq!(session.document().unwrap().selection(), selection);
    }

    // Uncertain native state stays terminal during collection too.
    for failure in [
        CaptureFailure::NativeUncertain,
        CaptureFailure::WorkerPanicked,
    ] {
        let (mut session, ticket) = collecting();
        assert_eq!(
            session.dispatch(CaptureAction::ScrollFrameCollected {
                ticket,
                result: Err(failure),
            }),
            vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ScrollRelaunchRequired
            ]
        );
        assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
    }

    // Display loss cancels the whole session, not just collection.
    let (mut session, ticket) = collecting();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameCollected {
            ticket,
            result: Err(CaptureFailure::DisplayInvalid),
        }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid)
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
}

#[test]
fn core_session_66_frames_after_cancel_or_completion_are_ignored() {
    let (mut session, ticket) = collecting();
    session.dispatch(CaptureAction::CancelScroll { ticket });
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    let frame = scroll_frame(&session);
    for action in [
        CaptureAction::ScrollFrameCollected {
            ticket,
            result: Ok(frame.clone()),
        },
        CaptureAction::ScrollStitchVerified {
            ticket,
            frame: frame.clone(),
            extended: true,
        },
        CaptureAction::ScrollStitchRefused {
            ticket,
            error: arcshot_core::ScrollStitchError::Ambiguous,
        },
    ] {
        assert_eq!(session.dispatch(action), vec![]);
    }
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

// CORE-SESSION: explicit partial-result choice after a known-state stop
// (I-11 step 11 / I-12 known vs uncertain). A usable verified result offers
// exactly accept-or-abandon; no usable result offers only abandon; uncertain
// native state and display loss never offer a partial.

/// Drives collection to `AwaitingPartialChoice` via one stitch refusal.
fn awaiting_partial() -> (CaptureSession, CaptureTicket) {
    let (mut session, ticket) = collecting();
    // Extend history first so the partial is larger than the origin frame.
    session.dispatch(CaptureAction::ScrollFrameCollected {
        ticket,
        result: Ok(scroll_frame(&session)),
    });
    session.dispatch(CaptureAction::ScrollStitchVerified {
        ticket,
        frame: stitched_frame(80),
        extended: true,
    });
    let effects = session.dispatch(CaptureAction::ScrollStitchRefused {
        ticket,
        error: arcshot_core::ScrollStitchError::Gap,
    });
    assert_eq!(
        effects,
        vec![CaptureEffect::ScrollPartialChoice(
            ticket,
            ScrollRefusal::Stitch(arcshot_core::ScrollStitchError::Gap)
        )]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
    (session, ticket)
}

fn partial_job(session: &mut CaptureSession, ticket: CaptureTicket) -> CaptureEffect {
    let effects = session.dispatch(CaptureAction::AcceptScrollPartial { ticket });
    effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("accepting a partial submits one compose job")
}

#[test]
fn core_session_67_stitch_refusal_with_verified_history_offers_partial_not_output() {
    let (mut session, ticket) = awaiting_partial();
    // No auto-output: completion is unreachable in the choice state.
    assert!(session.scroll_partial_available());
    assert_eq!(
        session
            .scroll_stitch_frame()
            .unwrap()
            .geometry()
            .pixel_size(),
        [200, 80]
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(!session.output_busy());
    assert_eq!(
        session.dispatch(CaptureAction::CompleteScroll { ticket }),
        vec![]
    );
    // The region selection is still the frozen document's selection.
    assert_eq!(session.document().unwrap().selection().unwrap().x(), 10.0);
    // Explicit accept reuses the shared fenced clipboard Finish for the
    // verified (extended) document, not a second output pipeline.
    let CaptureEffect::ComposeOutput(job) = partial_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 200, 80).unwrap());
    assert_eq!(job.target(), arcshot_core::OutputTarget::Clipboard);
    assert!(session.may_publish(&job));
    assert_eq!(
        session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Ok(())
        }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ShowCompletionPreview(ticket),
            CaptureEffect::PlayCompletionSound
        ]
    );
    // Any confirmed clipboard completion shares the same whole-image preview
    // post-processing, including an explicitly accepted verified part.
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(session.completion_preview().is_some());
}

#[test]
fn core_session_68_every_known_collecting_stop_offers_the_partial_choice() {
    // Content change, ambiguous overlap, geometry, target loss, input
    // protection loss and frame admission / budget failures while collecting.
    for refusal in [
        ScrollRefusal::Stitch(arcshot_core::ScrollStitchError::ContentChanged),
        ScrollRefusal::Stitch(arcshot_core::ScrollStitchError::Ambiguous),
        ScrollRefusal::Stitch(arcshot_core::ScrollStitchError::Geometry),
        ScrollRefusal::Target(ScrollTargetError::NoTarget),
        ScrollRefusal::Target(ScrollTargetError::Geometry),
        ScrollRefusal::Input(ScrollInputError::ProtectionLost),
        ScrollRefusal::Frame(ScrollFrameError::Admission),
        ScrollRefusal::Frame(ScrollFrameError::ResourceLimit),
    ] {
        let (mut session, ticket) = collecting();
        let action = match refusal {
            ScrollRefusal::Stitch(error) => CaptureAction::ScrollStitchRefused { ticket, error },
            ScrollRefusal::Target(error) => CaptureAction::ScrollTargetLost { ticket, error },
            ScrollRefusal::Input(ScrollInputError::ProtectionLost) => {
                CaptureAction::ScrollInputLost { ticket }
            }
            ScrollRefusal::Frame(ScrollFrameError::Admission) => {
                CaptureAction::ScrollFrameCollected {
                    ticket,
                    result: Err(CaptureFailure::InvalidFrame),
                }
            }
            ScrollRefusal::Frame(ScrollFrameError::ResourceLimit) => {
                CaptureAction::ScrollFrameCollected {
                    ticket,
                    result: Err(CaptureFailure::ResourceLimit),
                }
            }
            _ => panic!("matrix case is not a collecting stop: {refusal:?}"),
        };
        assert_eq!(
            session.dispatch(action),
            vec![CaptureEffect::ScrollPartialChoice(ticket, refusal.clone())],
            "case {refusal:?}"
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
        assert!(session.scroll_partial_available());
        assert!(!session.output_busy());
        assert_eq!(session.phase(), CapturePhase::Frozen);
    }
}

#[test]
fn core_session_69_resource_budget_stop_during_collecting_offers_the_partial() {
    let (mut session, ticket) = collecting();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollStitchRefused {
            ticket,
            error: arcshot_core::ScrollStitchError::ResourceLimit,
        }),
        vec![CaptureEffect::ScrollPartialChoice(
            ticket,
            ScrollRefusal::Stitch(arcshot_core::ScrollStitchError::ResourceLimit)
        )]
    );
    assert!(session.scroll_partial_available());
    // Accepting still goes through the budget-fenced shared output path.
    let CaptureEffect::ComposeOutput(job) = partial_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
}

#[test]
fn core_session_70_stops_before_collection_offer_only_abandon() {
    // Permissions missing.
    let (mut session, ticket) = frozen_selected();
    session.dispatch(CaptureAction::EnterScroll { ticket });
    let effects = session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: arcshot_core::ScrollPermissions {
            screen_recording: true,
            input_monitoring: false,
            accessibility: true,
        },
    });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CaptureEffect::ScrollRefused(_, _))),
        "permission refusal must not offer a partial: {effects:?}"
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(!session.scroll_partial_available());
    assert_eq!(
        session.dispatch(CaptureAction::AcceptScrollPartial { ticket }),
        vec![]
    );
    // Initial-frame failures (admission / geometry / budget / worker).
    for failure in [
        CaptureFailure::InvalidFrame,
        CaptureFailure::ResourceLimit,
        CaptureFailure::WorkerUnavailable,
    ] {
        let (mut session, ticket) = prepared();
        session.dispatch(CaptureAction::ScrollInputInstalled {
            ticket,
            result: Ok(()),
        });
        let effects = session.dispatch(CaptureAction::ScrollFrameAdmitted {
            ticket,
            result: Err(failure),
        });
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, CaptureEffect::ScrollRefused(_, _))),
            "initial-frame refusal must not offer a partial: {effects:?}"
        );
        assert!(!session.scroll_partial_available());
        assert_eq!(
            session.dispatch(CaptureAction::AcceptScrollPartial { ticket }),
            vec![]
        );
    }
}

#[test]
fn core_session_71_abandon_returns_the_pre_scroll_selection_and_clears_the_partial() {
    let (mut session, ticket) = awaiting_partial();
    let selection = session.document().unwrap().selection().unwrap();
    let version = session.document().unwrap().version();
    assert_eq!(
        session.dispatch(CaptureAction::AbandonScrollPartial { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(!session.scroll_partial_available());
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection(), Some(selection));
    assert_eq!(session.document().unwrap().version(), version);
    // Late partial / completion / stitch results are ignored after abandon.
    let frame = scroll_frame(&session);
    for action in [
        CaptureAction::AcceptScrollPartial { ticket },
        CaptureAction::CompleteScroll { ticket },
        CaptureAction::ScrollStitchVerified {
            ticket,
            frame: frame.clone(),
            extended: true,
        },
        CaptureAction::ScrollFrameCollected {
            ticket,
            result: Ok(frame),
        },
    ] {
        assert_eq!(session.dispatch(action), vec![]);
    }
    // The user may enter Scroll again from the returned selection.
    session.dispatch(CaptureAction::EnterScroll { ticket });
    assert_eq!(session.scroll_phase(), ScrollPhase::CheckingPermissions);
}

#[test]
fn core_session_72_failed_partial_output_keeps_the_result_for_retry_or_abandon() {
    for failure in [
        arcshot_core::OutputFailure::Clipboard,
        arcshot_core::OutputFailure::Encode,
        arcshot_core::OutputFailure::Worker,
        arcshot_core::OutputFailure::ResourceLimit,
    ] {
        let (mut session, ticket) = awaiting_partial();
        let CaptureEffect::ComposeOutput(job) = partial_job(&mut session, ticket) else {
            panic!("expected one compose job");
        };
        let effects = session.dispatch(CaptureAction::OutputCompleted {
            job,
            result: Err(failure),
        });
        // No success: the session is not resident, no sound, choice returns.
        assert!(
            effects.contains(&CaptureEffect::ReportOutputFailure(failure)),
            "{effects:?}"
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, CaptureEffect::ScrollPartialChoice(_, _))),
            "failed partial output must re-offer the choice: {effects:?}"
        );
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
        assert!(session.scroll_partial_available());
        assert!(!session.output_busy());
        // Retry accept succeeds through the same job path.
        let CaptureEffect::ComposeOutput(job) = partial_job(&mut session, ticket) else {
            panic!("retry submits a fresh compose job");
        };
        assert_eq!(
            session.dispatch(CaptureAction::OutputCompleted {
                job,
                result: Ok(())
            }),
            vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ShowCompletionPreview(ticket),
                CaptureEffect::PlayCompletionSound
            ]
        );
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert!(session.completion_preview().is_some());
    }
    // Abandon after a failed accept also returns the original selection.
    let (mut session, ticket) = awaiting_partial();
    let CaptureEffect::ComposeOutput(job) = partial_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Err(arcshot_core::OutputFailure::Clipboard),
    });
    assert_eq!(
        session.dispatch(CaptureAction::AbandonScrollPartial { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert_eq!(session.document().unwrap().selection().unwrap().x(), 10.0);
}

#[test]
fn core_session_73_explicit_cancel_during_collecting_never_offers_a_partial() {
    let (mut session, ticket) = collecting();
    assert_eq!(
        session.dispatch(CaptureAction::CancelScroll { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(!session.scroll_partial_available());
    assert_eq!(
        session.dispatch(CaptureAction::AcceptScrollPartial { ticket }),
        vec![]
    );
    // Esc while awaiting the choice is the same abandon.
    let (mut session, ticket) = awaiting_partial();
    assert_eq!(
        session.dispatch(CaptureAction::CancelScroll { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
}

#[test]
fn core_session_74_uncertain_native_state_and_display_loss_never_offer_a_partial() {
    // Uncertain state during collection is terminal, even with history.
    for failure in [
        CaptureFailure::NativeUncertain,
        CaptureFailure::WorkerPanicked,
    ] {
        let (mut session, ticket) = collecting();
        assert_eq!(
            session.dispatch(CaptureAction::ScrollFrameCollected {
                ticket,
                result: Err(failure),
            }),
            vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ScrollRelaunchRequired
            ]
        );
        assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
        assert!(!session.scroll_partial_available());
        assert_eq!(
            session.dispatch(CaptureAction::Start),
            vec![CaptureEffect::ReportFailure(
                CaptureFailure::NativeUncertain
            )]
        );
    }
    // Display loss cancels the whole session.
    let (mut session, ticket) = collecting();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollFrameCollected {
            ticket,
            result: Err(CaptureFailure::DisplayInvalid),
        }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid)
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert!(!session.scroll_partial_available());
    // Uncertain state arriving while awaiting the choice discards it.
    let (mut session, ticket) = awaiting_partial();
    assert_eq!(
        session.dispatch(CaptureAction::ScrollNativeUncertain { ticket }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ScrollRelaunchRequired
        ]
    );
    assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
    assert!(!session.scroll_partial_available());
    // Display loss while awaiting also discards it.
    let (mut session, _ticket) = awaiting_partial();
    assert_eq!(
        session.dispatch(CaptureAction::DisplayInvalidated),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid)
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert!(!session.scroll_partial_available());
}

#[test]
fn core_session_75_hotkey_restart_and_reentry_cannot_steal_or_pollute_the_choice() {
    // Hotkey restart during the choice passes the native exit fence and
    // clears the partial; late accept with the old ticket is ignored.
    let (mut session, ticket) = awaiting_partial();
    let effects = session.dispatch(CaptureAction::Start);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CaptureEffect::CheckPermission(_))),
        "{effects:?}"
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(!session.scroll_partial_available());
    assert_eq!(
        session.dispatch(CaptureAction::AcceptScrollPartial { ticket }),
        vec![]
    );
    // Re-entering Scroll while awaiting must not overwrite the partial
    // without an explicit abandon.
    let (mut session, ticket) = awaiting_partial();
    assert_eq!(
        session.dispatch(CaptureAction::EnterScroll { ticket }),
        vec![]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::AwaitingPartialChoice);
    assert!(session.scroll_partial_available());
}

#[test]
fn core_session_76_late_results_after_partial_success_never_output_again() {
    let (mut session, ticket) = awaiting_partial();
    let CaptureEffect::ComposeOutput(job) = partial_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job: job.clone(),
        result: Ok(()),
    });
    // A successful accepted partial now enters the shared completion preview,
    // so the session stays frozen and awaits annotation or cancellation.
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(session.completion_preview().is_some());
    // The same ticket's late actions publish nothing in either state.
    for action in [
        CaptureAction::AcceptScrollPartial { ticket },
        CaptureAction::AbandonScrollPartial { ticket },
        CaptureAction::CompleteScroll { ticket },
        CaptureAction::OutputCompleted {
            job,
            result: Ok(()),
        },
    ] {
        assert_eq!(session.dispatch(action), vec![]);
    }
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(session.completion_preview().is_some());
}

#[test]
fn core_session_77_cancel_after_the_completion_preview_ends_without_output() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Ok(()),
    });
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(session.completion_preview().is_some());
    let version = session.document().unwrap().version();

    // Cancelling the annotation ends the session without a second job: the
    // clipboard keeps the un-annotated copy that already succeeded.
    assert_eq!(
        session.dispatch(CaptureAction::CancelAttempt(ticket)),
        vec![CaptureEffect::DismissOverlay]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert!(session.document().is_none());
    assert!(session.completion_preview().is_none());
    assert_eq!(version, 1, "no annotation was committed before cancel");
}
#[test]
fn core_session_78_re_finish_outputs_the_whole_annotated_image_and_then_ends() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Ok(()),
    });

    // Clicking the preview consumes the post-copy marker and enters the same
    // document; no second annotator or re-selection is created.
    assert_eq!(
        session.dispatch(CaptureAction::EnterCompletionAnnotation { ticket }),
        vec![CaptureEffect::PresentCompletionAnnotation(ticket)]
    );
    assert!(session.completion_preview().is_none());

    // Draw one Rectangle over the top-left of the long image with the existing
    // editor.
    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Rectangle,
    });
    session.dispatch(CaptureAction::PointerPressed {
        ticket,
        point: [2.0, 2.0],
    });
    session.dispatch(CaptureAction::PointerMoved {
        ticket,
        point: [8.0, 9.0],
    });
    session.dispatch(CaptureAction::PointerReleased { ticket });
    let document = session.document().unwrap();
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(document.version(), 2);

    // Re-finishing snapshots the full frame plus the annotation.
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let effect = effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("one compose job");
    let CaptureEffect::ComposeOutput(job) = effect else {
        panic!("expected one compose job");
    };
    assert_eq!(job.ticket(), ticket);
    assert_eq!(job.target(), arcshot_core::OutputTarget::Clipboard);
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
    assert_eq!(job.annotations().len(), 1);
    assert!(session.may_publish(&job));

    // A successful re-finish ends the session: there is no second preview.
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
    assert!(session.completion_preview().is_none());
}
#[test]
fn core_session_79_ordinary_region_finish_still_ends_without_a_preview() {
    let (mut session, ticket) = frozen_selected();
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let effect = effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("one compose job");
    let CaptureEffect::ComposeOutput(job) = effect else {
        panic!("expected one compose job");
    };
    assert!(session.completion_preview().is_none());
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
fn core_session_80_re_finish_without_edits_still_ends_the_session() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Ok(()),
    });
    session.dispatch(CaptureAction::EnterCompletionAnnotation { ticket });
    assert!(session.completion_preview().is_none());

    // Even an untouched annotation surface finishes normally: the whole frame
    // is copied and the session ends instead of showing another preview.
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let effect = effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("one compose job");
    let CaptureEffect::ComposeOutput(job) = effect else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
    assert!(job.annotations().is_empty());
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
fn core_session_81_finish_during_the_preview_is_a_re_finish_not_a_second_preview() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Ok(()),
    });
    assert!(session.completion_preview().is_some());

    // Pressing Finish while the thumbnail is visible copies the same whole
    // image again and ends the session; it must not restart the preview.
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let effect = effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("finish submits one compose job");
    let CaptureEffect::ComposeOutput(job) = effect else {
        panic!("expected one compose job");
    };
    assert_eq!(job.crop(), PixelRect::try_new(0, 0, 60, 40).unwrap());
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
fn core_session_82_accessibility_rectangle_enters_the_shared_document() {
    let (mut session, ticket) = collecting();
    let CaptureEffect::ComposeOutput(job) = completion_job(&mut session, ticket) else {
        panic!("expected one compose job");
    };
    session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Ok(()),
    });
    session.dispatch(CaptureAction::EnterCompletionAnnotation { ticket });

    // The non-drag accessibility path creates one Rectangle through the same
    // authoritative editor at a deterministic inset of the whole image.
    assert_eq!(
        session.dispatch(CaptureAction::AddDefaultRectangle { ticket }),
        vec![]
    );
    let document = session.document().unwrap();
    assert_eq!(document.annotations().len(), 1);
    assert_eq!(document.version(), 2);
    let bounds = document.annotations()[0]
        .shape()
        .rectangle_bounds()
        .expect("one rectangle object");
    assert_eq!(bounds.x(), 3.0);
    assert_eq!(bounds.y(), 2.0);
    assert_eq!(bounds.width(), 6.0);
    assert_eq!(bounds.height(), 4.0);

    // While an output runs the document is fenced from further edits.
    let effects = session.dispatch(CaptureAction::Finish { ticket });
    let effect = effects
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("finish submits one compose job");
    let CaptureEffect::ComposeOutput(job) = effect else {
        panic!("expected one compose job");
    };
    assert_eq!(job.annotations().len(), 1);
    assert_eq!(
        session.dispatch(CaptureAction::AddDefaultRectangle { ticket }),
        vec![]
    );
    assert_eq!(session.document().unwrap().annotations().len(), 1);
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
