//! APP-ASYNC: old overlay events cannot cancel a newer Capture Session.
use arcshot_core::{
    CaptureAction as A, CaptureEffect as E, CapturePhase, CaptureSession, CapturedFrame,
    ScreenGeometry,
};
#[test]
fn app_async_08_old_escape_processed_after_restart_does_not_cancel_new_attempt() {
    let mut session = CaptureSession::default();
    let E::CheckPermission(old) = session.dispatch(A::Start)[0] else {
        panic!("expected preflight");
    };
    session.dispatch(A::PermissionChecked {
        ticket: old,
        available: true,
    });
    let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap();
    let frame = CapturedFrame::try_from_srgb_bgra(g, 2, 2, 8, vec![255; 16]).unwrap();
    session.dispatch(A::Completed {
        ticket: old,
        result: Ok(frame),
    });
    let effects = session.dispatch(A::Start);
    let new = effects
        .into_iter()
        .find_map(|e| {
            if let E::CheckPermission(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .unwrap();
    session.dispatch(A::PermissionChecked {
        ticket: new,
        available: true,
    });
    // Same app tick: native StartCapture was processed before old overlay's Esc.
    assert!(session.dispatch(A::CancelAttempt(old)).is_empty());
    assert!(session.may_capture(new));
    assert_eq!(session.phase(), CapturePhase::Preparing);
    assert_eq!(
        session.dispatch(A::CancelAttempt(new)),
        vec![E::DismissOverlay]
    );
    assert_eq!(session.phase(), CapturePhase::Draining);
}

#[test]
fn app_async_09_dismissed_preparing_overlay_cannot_erase_a_queued_restart() {
    for completion in [Ok(()), Err(arcshot_core::CaptureFailure::NativeUncertain)] {
        let mut session = CaptureSession::default();
        let E::CheckPermission(old) = session.dispatch(A::Start)[0] else {
            panic!("expected preflight");
        };
        session.dispatch(A::PermissionChecked {
            ticket: old,
            available: true,
        });
        session.dispatch(A::Start);
        assert_eq!(session.phase(), CapturePhase::Draining);
        assert!(session.dispatch(A::CancelAttempt(old)).is_empty());
        let frame = || {
            let g = ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap();
            CapturedFrame::try_from_srgb_bgra(g, 2, 2, 8, vec![255; 16]).unwrap()
        };
        let effects = session.dispatch(A::Completed {
            ticket: old,
            result: completion.map(|()| frame()),
        });
        if completion.is_ok() {
            assert!(matches!(effects.as_slice(),[E::CheckPermission(new)] if *new!=old));
            assert_eq!(session.phase(), CapturePhase::CheckingPermission);
        } else {
            assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
            assert!(
                effects
                    .iter()
                    .all(|effect| !matches!(effect, E::Capture(_) | E::CheckPermission(_)))
            );
        }
    }
}
