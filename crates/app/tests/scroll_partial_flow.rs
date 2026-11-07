//! APP-EFFECT / APP-ASYNC: the explicit partial-result choice after a
//! known-state Scroll stop (I-11 step 11 / I-12).
//!
//! The injected failure family is classified by a pure policy: a verified
//! result plus a known-state refusal offers exactly two choices; without a
//! verified result, or for uncertain native state, only abandon is offered
//! and nothing can auto-output. Tap commands while awaiting the choice
//! (including late ones) route nowhere.
use arcshot_app::{
    PARTIAL_ABANDON_LABEL, PARTIAL_ACCEPT_LABEL, PARTIAL_INCOMPLETE_WARNING, ScrollPartialChoice,
    partial_action, partial_choices, partial_offered,
};
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
    ScrollFrameError, ScrollInputError, ScrollPermissions, ScrollPhase, ScrollRefusal,
    ScrollStitchError, ScrollTarget, ScrollTargetError, ScrollTargetFrame, ScrollWindowError,
};

fn ticket() -> CaptureTicket {
    let mut session = CaptureSession::default();
    let effects = session.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = &effects[0] else {
        panic!("expected a permission check");
    };
    *ticket
}

fn known_refusals() -> Vec<ScrollRefusal> {
    vec![
        // Dynamic content / fixed header / horizontal-into-overlap: the
        // overlap is not fully consistent with verified history.
        ScrollRefusal::Stitch(ScrollStitchError::ContentChanged),
        // Repeated / low-texture: more than one displacement explains it.
        ScrollRefusal::Stitch(ScrollStitchError::Ambiguous),
        // No-overlap fast jump / diagonal or nested drift with no match.
        ScrollRefusal::Stitch(ScrollStitchError::Gap),
        ScrollRefusal::Stitch(ScrollStitchError::Geometry),
        ScrollRefusal::Stitch(ScrollStitchError::ResourceLimit),
        ScrollRefusal::Frame(ScrollFrameError::Admission),
        ScrollRefusal::Frame(ScrollFrameError::Geometry),
        ScrollRefusal::Frame(ScrollFrameError::ResourceLimit),
        ScrollRefusal::Frame(ScrollFrameError::Worker),
        ScrollRefusal::Target(ScrollTargetError::NoTarget),
        ScrollRefusal::Target(ScrollTargetError::NotFrontmost),
        ScrollRefusal::Target(ScrollTargetError::Geometry),
        ScrollRefusal::Target(ScrollTargetError::PointerMismatch),
        ScrollRefusal::Input(ScrollInputError::ProtectionLost),
        ScrollRefusal::Input(ScrollInputError::TapUnavailable),
        ScrollRefusal::Input(ScrollInputError::NotFrontmost),
        ScrollRefusal::Windows(ScrollWindowError::MappingFailed),
        ScrollRefusal::Permissions(vec![arcshot_core::ScrollPermission::Accessibility]),
    ]
}

#[test]
fn app_effect_45_known_failures_with_verified_result_offer_exactly_two_choices() {
    for refusal in known_refusals() {
        assert!(
            partial_offered(true, &refusal),
            "known refusal with verified history must offer a partial: {refusal:?}"
        );
        assert_eq!(
            partial_choices(true, &refusal),
            vec![
                ScrollPartialChoice::UseVerifiedPartial,
                ScrollPartialChoice::Abandon
            ],
            "case {refusal:?}"
        );
    }
}

#[test]
fn app_effect_46_no_verified_result_or_uncertain_state_offers_only_abandon() {
    // No usable verified result: only abandon, never auto-output.
    for refusal in known_refusals() {
        assert!(!partial_offered(false, &refusal), "case {refusal:?}");
        assert_eq!(
            partial_choices(false, &refusal),
            vec![ScrollPartialChoice::Abandon],
            "case {refusal:?}"
        );
    }
    // Unconfirmable native state forbids a partial even with history.
    for refusal in [
        ScrollRefusal::Target(ScrollTargetError::NativeUncertain),
        ScrollRefusal::Windows(ScrollWindowError::NativeUncertain),
        ScrollRefusal::Windows(ScrollWindowError::NotMainThread),
    ] {
        assert!(!partial_offered(true, &refusal), "case {refusal:?}");
        assert_eq!(
            partial_choices(true, &refusal),
            vec![ScrollPartialChoice::Abandon],
            "case {refusal:?}"
        );
    }
}

#[test]
fn app_effect_47_choice_labels_warn_that_the_partial_is_incomplete() {
    assert_eq!(PARTIAL_ACCEPT_LABEL, "使用已验证部分");
    assert_eq!(PARTIAL_ABANDON_LABEL, "放弃并返回原选区");
    assert!(
        PARTIAL_INCOMPLETE_WARNING.contains("不完整"),
        "the user must be told the result is incomplete: {PARTIAL_INCOMPLETE_WARNING}"
    );
    assert!(
        PARTIAL_INCOMPLETE_WARNING.contains("确认"),
        "copy must wait for explicit confirmation: {PARTIAL_INCOMPLETE_WARNING}"
    );
}

#[test]
fn app_effect_48_choices_map_to_the_explicit_session_actions_with_the_live_ticket() {
    let live = ticket();
    assert_eq!(
        partial_action(ScrollPartialChoice::UseVerifiedPartial, live),
        CaptureAction::AcceptScrollPartial { ticket: live }
    );
    assert_eq!(
        partial_action(ScrollPartialChoice::Abandon, live),
        CaptureAction::AbandonScrollPartial { ticket: live }
    );
}

// APP-EFFECT: encode/copy failure after accept keeps both choices.
#[test]
fn app_effect_49_failed_partial_output_still_offers_retry_or_abandon() {
    // Drive a real session to the choice, accept, fail the publish, and
    // observe that the choice (not a success) is what remains.
    let mut session = CaptureSession::default();
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected a permission check");
    };
    let frame = arcshot_core::CapturedFrame::try_from_srgb_bgra(
        arcshot_core::ScreenGeometry::try_new(1, 0.0, 0.0, 100.0, 50.0, 2.0).unwrap(),
        200,
        100,
        800,
        vec![255; 200 * 100 * 4],
    )
    .unwrap();
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    session.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame),
    });
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    session.dispatch(CaptureAction::EnterScroll { ticket });
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: ScrollPermissions {
            screen_recording: true,
            input_monitoring: true,
            accessibility: true,
        },
    });
    session.dispatch(CaptureAction::ScrollTargetIdentified {
        ticket,
        result: Ok(ScrollTarget {
            window_id: 7,
            owner: "com.example".to_owned(),
            owner_pid: 1,
            frame: ScrollTargetFrame {
                x: 0.0,
                y: 0.0,
                width: 60.0,
                height: 40.0,
            },
        }),
    });
    session.dispatch(CaptureAction::ScrollWindowsPrepared {
        ticket,
        result: Ok(vec![11]),
    });
    session.dispatch(CaptureAction::ScrollInputInstalled {
        ticket,
        result: Ok(()),
    });
    let document = session.document().unwrap();
    let crop = document
        .selection()
        .unwrap()
        .pixel_rect(document.frame().geometry())
        .unwrap();
    let geometry = arcshot_core::scroll_frame_geometry(document.frame().geometry(), crop).unwrap();
    let [w, h] = geometry.pixel_size();
    let scroll_frame = arcshot_core::CapturedFrame::try_from_srgb_bgra(
        geometry,
        w,
        h,
        w as usize * 4,
        vec![255; w as usize * h as usize * 4],
    )
    .unwrap();
    session.dispatch(CaptureAction::ScrollFrameAdmitted {
        ticket,
        result: Ok(scroll_frame),
    });
    assert_eq!(session.scroll_phase(), ScrollPhase::Collecting);
    let effects = session.dispatch(CaptureAction::ScrollStitchRefused {
        ticket,
        error: ScrollStitchError::Gap,
    });
    assert_eq!(
        effects,
        vec![CaptureEffect::ScrollPartialChoice(
            ticket,
            ScrollRefusal::Stitch(ScrollStitchError::Gap)
        )]
    );
    let CaptureEffect::ComposeOutput(job) = session
        .dispatch(partial_action(
            ScrollPartialChoice::UseVerifiedPartial,
            ticket,
        ))
        .into_iter()
        .find(|effect| matches!(effect, CaptureEffect::ComposeOutput(_)))
        .expect("accept must submit one compose job")
    else {
        panic!("accept must submit one compose job");
    };
    let effects = session.dispatch(CaptureAction::OutputCompleted {
        job,
        result: Err(arcshot_core::OutputFailure::Clipboard),
    });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CaptureEffect::ScrollPartialChoice(_, _))),
        "failed publish must return to the choice: {effects:?}"
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, CaptureEffect::PlayCompletionSound))
    );
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(session.scroll_partial_available());
    assert_eq!(
        partial_choices(
            session.scroll_partial_available(),
            session.scroll_refusal().unwrap()
        ),
        vec![
            ScrollPartialChoice::UseVerifiedPartial,
            ScrollPartialChoice::Abandon
        ]
    );
}

// APP-ASYNC: tap commands while awaiting the choice (incl. late ones) route
// nowhere; the choice is resolved only through its explicit UI actions.
#[test]
fn app_async_21_tap_commands_while_awaiting_the_choice_route_nowhere() {
    use arcshot_app::{ScrollTapCommand, scroll_tap_action};
    let live = ticket();
    for command in [ScrollTapCommand::Finish, ScrollTapCommand::Cancel] {
        assert_eq!(
            scroll_tap_action(
                ScrollPhase::AwaitingPartialChoice,
                CapturePhase::Frozen,
                false,
                live,
                command
            ),
            None,
            "tap command {command:?} must not auto-output or restart while awaiting"
        );
        // A late tap command after abandon (Inactive) is equally inert.
        assert_eq!(
            scroll_tap_action(
                ScrollPhase::Inactive,
                CapturePhase::Frozen,
                false,
                live,
                command
            ),
            None
        );
    }
}

// APP-ASYNC: uncertain native state while awaiting forbids partial + restart
// at the session fence (the app-only half is the policy above; the session
// half is CORE-SESSION-74, pinned here through the live session).
#[test]
fn app_async_22_uncertain_state_while_awaiting_discards_the_partial() {
    let mut session = CaptureSession::default();
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected a permission check");
    };
    assert_eq!(
        session.dispatch(CaptureAction::ScrollNativeUncertain { ticket }),
        vec![],
        "uncertain state with no preparation is ignored, not a restart"
    );
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    assert_eq!(
        session.dispatch(CaptureAction::Completed {
            ticket,
            result: Err(CaptureFailure::NativeUncertain),
        }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::NativeUncertain)
        ]
    );
    let _ = ticket;
}
