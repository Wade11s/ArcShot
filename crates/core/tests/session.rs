//! CORE-SESSION and APP-ASYNC actions and externally visible effects.
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
    CapturedFrame, ScreenGeometry,
};

fn start(session: &mut CaptureSession) -> CaptureTicket {
    let effects = session.dispatch(CaptureAction::Start);
    let ticket = effects
        .iter()
        .find_map(|e| match e {
            CaptureEffect::CheckPermission(t) => Some(*t),
            _ => None,
        })
        .unwrap();
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    ticket
}
fn frame() -> CapturedFrame {
    let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap();
    CapturedFrame::try_from_srgb_bgra(g, 2, 2, 8, vec![255; 16]).unwrap()
}
#[test]
fn core_session_01_start_preflights_permission_before_capture() {
    let mut session = CaptureSession::default();
    let effects = session.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = effects[0] else {
        panic!("expected permission check");
    };
    assert_eq!(session.phase(), CapturePhase::CheckingPermission);
    assert_eq!(
        session.dispatch(CaptureAction::PermissionChecked {
            ticket,
            available: true
        }),
        vec![CaptureEffect::Capture(ticket)]
    );
    assert_eq!(session.phase(), CapturePhase::Preparing);
}
#[test]
fn core_session_02_success_freezes_then_escape_releases_without_output() {
    let mut s = CaptureSession::default();
    let ticket = start(&mut s);
    assert_eq!(
        s.dispatch(CaptureAction::Completed {
            ticket,
            result: Ok(frame())
        }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert!(s.frame().is_some());
    assert!(s.may_present(ticket));
    assert_eq!(
        s.dispatch(CaptureAction::Cancel),
        vec![CaptureEffect::DismissOverlay]
    );
    assert_eq!(s.phase(), CapturePhase::Resident);
    assert!(s.frame().is_none());
    assert!(!s.may_present(ticket));
}
#[test]
fn app_async_01_cancel_waits_for_safe_native_completion_and_discards_frame() {
    let mut s = CaptureSession::default();
    let ticket = start(&mut s);
    s.dispatch(CaptureAction::Cancel);
    assert_eq!(s.phase(), CapturePhase::Draining);
    assert!(!s.may_capture(ticket));
    assert!(
        s.dispatch(CaptureAction::Completed {
            ticket,
            result: Ok(frame())
        })
        .is_empty()
    );
    assert_eq!(s.phase(), CapturePhase::Resident);
    assert!(s.frame().is_none());
}
#[test]
fn app_async_02_restart_does_not_issue_a_second_request_until_worker_finishes() {
    let mut s = CaptureSession::default();
    let ticket = start(&mut s);
    assert_eq!(
        s.dispatch(CaptureAction::Start),
        vec![CaptureEffect::DismissOverlay]
    );
    assert_eq!(s.phase(), CapturePhase::Draining);
    let effects = s.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    let CaptureEffect::CheckPermission(new) = effects[0] else {
        panic!("expected new preflight");
    };
    assert_ne!(new, ticket);
    assert!(
        s.dispatch(CaptureAction::Completed {
            ticket,
            result: Ok(frame())
        })
        .is_empty()
    );
    assert!(s.frame().is_none());
}
#[test]
fn app_async_03_uncertain_failure_is_terminal_even_after_cancel() {
    for failure in [
        CaptureFailure::NativeUncertain,
        CaptureFailure::WorkerPanicked,
    ] {
        let mut s = CaptureSession::default();
        let ticket = start(&mut s);
        s.dispatch(CaptureAction::Cancel);
        assert_eq!(
            s.dispatch(CaptureAction::Completed {
                ticket,
                result: Err(failure)
            }),
            vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ReportFailure(failure)
            ]
        );
        assert_eq!(s.phase(), CapturePhase::RelaunchRequired);
        assert!(s.frame().is_none());
        assert_eq!(
            s.dispatch(CaptureAction::Start),
            vec![CaptureEffect::ReportFailure(
                CaptureFailure::NativeUncertain
            )]
        );
        assert!(
            s.dispatch(CaptureAction::Completed {
                ticket,
                result: Ok(frame())
            })
            .is_empty()
        );
        assert_eq!(s.phase(), CapturePhase::RelaunchRequired);
    }
}
#[test]
fn core_session_03_unavailable_permission_does_not_enter_capture() {
    let mut s = CaptureSession::default();
    let effects = s.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = effects[0] else {
        panic!("expected check");
    };
    assert_eq!(
        s.dispatch(CaptureAction::PermissionChecked {
            ticket,
            available: false
        }),
        vec![CaptureEffect::PermissionUnavailable]
    );
    assert_eq!(s.phase(), CapturePhase::Resident);
    assert!(
        s.dispatch(CaptureAction::Start)
            .iter()
            .any(|e| matches!(e, CaptureEffect::CheckPermission(_)))
    );
}
#[test]
fn core_session_04_display_invalidation_drops_frame_and_never_migrates() {
    let mut s = CaptureSession::default();
    let ticket = start(&mut s);
    s.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    assert_eq!(
        s.dispatch(CaptureAction::DisplayInvalidated),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid)
        ]
    );
    assert!(s.frame().is_none());
    assert!(!s.may_present(ticket));
}
#[test]
fn core_session_05_known_frame_failure_can_retry() {
    let mut s = CaptureSession::default();
    let ticket = start(&mut s);
    s.dispatch(CaptureAction::Completed {
        ticket,
        result: Err(CaptureFailure::InvalidFrame),
    });
    assert_eq!(s.phase(), CapturePhase::Resident);
    assert_ne!(start(&mut s), ticket);
}
#[test]
fn app_async_04_cancelled_preflight_cannot_submit_native_work() {
    let mut s = CaptureSession::default();
    let effects = s.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = effects[0] else {
        panic!("expected check");
    };
    s.dispatch(CaptureAction::Cancel);
    assert!(
        s.dispatch(CaptureAction::PermissionChecked {
            ticket,
            available: true
        })
        .is_empty()
    );
    assert!(!s.may_capture(ticket));
}
