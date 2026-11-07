//! CORE-SESSION: Scroll Capture entry, permissions, target lock and terminal states.
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
    CapturedFrame, ScreenGeometry, ScrollPermission, ScrollPermissions, ScrollPhase, ScrollRefusal,
    ScrollTarget, ScrollTargetError, ScrollTargetFrame, ScrollWindowError, Tool,
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
    assert_eq!(
        session.dispatch(CaptureAction::Completed {
            ticket,
            result: Ok(frame())
        }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    (session, ticket)
}

fn enter(session: &mut CaptureSession, ticket: CaptureTicket) -> Vec<CaptureEffect> {
    session.dispatch(CaptureAction::EnterScroll { ticket })
}

fn permissions(
    screen_recording: bool,
    input_monitoring: bool,
    accessibility: bool,
) -> ScrollPermissions {
    ScrollPermissions {
        screen_recording,
        input_monitoring,
        accessibility,
    }
}

fn target(frame: ScrollTargetFrame) -> ScrollTarget {
    ScrollTarget {
        window_id: 4242,
        owner: "com.example.target".to_owned(),
        owner_pid: 999,
        frame,
    }
}

#[test]
fn core_session_28_enter_scroll_requires_a_valid_selection() {
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
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(
        enter(&mut session, ticket),
        vec![CaptureEffect::ScrollRefused(
            ticket,
            ScrollRefusal::NoSelection
        )]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(session.document().unwrap().selection().is_none());
}

#[test]
fn core_session_29_enter_scroll_refuses_annotations_without_dropping_them() {
    let (mut session, ticket) = frozen_selected();
    // One committed rectangle annotation inside the selection.
    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Rectangle,
    });
    session.dispatch(CaptureAction::PointerPressed {
        ticket,
        point: [12.0, 8.0],
    });
    session.dispatch(CaptureAction::PointerMoved {
        ticket,
        point: [20.0, 15.0],
    });
    session.dispatch(CaptureAction::PointerReleased { ticket });
    assert_eq!(session.document().unwrap().annotations().len(), 1);
    let version = session.document().unwrap().version();

    assert_eq!(
        enter(&mut session, ticket),
        vec![CaptureEffect::ScrollRefused(
            ticket,
            ScrollRefusal::AnnotationsPresent
        )]
    );
    // The refusal never silently drops or migrates the annotation.
    assert_eq!(session.document().unwrap().annotations().len(), 1);
    assert_eq!(session.document().unwrap().version(), version);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_30_enter_scroll_reuses_the_selection_and_checks_three_permissions() {
    let (mut session, ticket) = frozen_selected();
    let selection = session.document().unwrap().selection().unwrap();
    assert_eq!(
        enter(&mut session, ticket),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::CheckScrollPermissions(ticket)
        ]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::CheckingPermissions);
    assert_eq!(session.scroll_selection(), Some(selection));
    // The region session and its selection are unchanged; no second capture.
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection(), Some(selection));
    assert_eq!(session.scroll_exclusion(), None);
}

#[test]
fn core_session_31_missing_scroll_permissions_stop_and_keep_the_region_session() {
    let (mut session, ticket) = frozen_selected();
    let selection = session.document().unwrap().selection().unwrap();
    enter(&mut session, ticket);
    let missing = vec![
        ScrollPermission::InputMonitoring,
        ScrollPermission::Accessibility,
    ];
    assert_eq!(
        session.dispatch(CaptureAction::ScrollPermissionsChecked {
            ticket,
            available: permissions(true, false, false)
        }),
        vec![
            CaptureEffect::ScrollRefused(ticket, ScrollRefusal::Permissions(missing.clone())),
            CaptureEffect::PresentFrozen(ticket)
        ]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection(), Some(selection));
    assert!(!session.output_busy());
}

#[test]
fn core_session_32_all_permissions_proceed_to_target_identification() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    assert_eq!(
        session.dispatch(CaptureAction::ScrollPermissionsChecked {
            ticket,
            available: permissions(true, true, true)
        }),
        vec![CaptureEffect::IdentifyScrollTarget(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::IdentifyingTarget);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_33_target_lock_requires_the_selection_inside_the_window() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    // A window that does not contain the whole selection is a geometry refusal.
    assert_eq!(
        session.dispatch(CaptureAction::ScrollTargetIdentified {
            ticket,
            result: Ok(target(ScrollTargetFrame {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0
            }))
        }),
        vec![
            CaptureEffect::ScrollRefused(
                ticket,
                ScrollRefusal::Target(ScrollTargetError::Geometry)
            ),
            CaptureEffect::PresentFrozen(ticket)
        ]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);

    // A containing window locks the public identity and proceeds.
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    let locked = target(ScrollTargetFrame {
        x: 0.0,
        y: 0.0,
        width: 60.0,
        height: 40.0,
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollTargetIdentified {
            ticket,
            result: Ok(locked.clone())
        }),
        vec![CaptureEffect::PrepareScrollWindows(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::PreparingWindows);
    assert_eq!(session.scroll_target(), Some(&locked));
}

#[test]
fn core_session_34_known_target_failures_stop_and_keep_the_region_session() {
    for error in [
        ScrollTargetError::NoTarget,
        ScrollTargetError::AmbiguousTarget,
        ScrollTargetError::NotFrontmost,
        ScrollTargetError::PointerMismatch,
    ] {
        let (mut session, ticket) = frozen_selected();
        enter(&mut session, ticket);
        session.dispatch(CaptureAction::ScrollPermissionsChecked {
            ticket,
            available: permissions(true, true, true),
        });
        assert_eq!(
            session.dispatch(CaptureAction::ScrollTargetIdentified {
                ticket,
                result: Err(error)
            }),
            vec![
                CaptureEffect::ScrollRefused(ticket, ScrollRefusal::Target(error)),
                CaptureEffect::PresentFrozen(ticket)
            ]
        );
        assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
        assert_eq!(session.phase(), CapturePhase::Frozen);
        assert!(session.document().unwrap().selection().is_some());
    }
}

#[test]
fn core_session_35_window_exclusion_mapping_failure_refuses_without_output() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    session.dispatch(CaptureAction::ScrollTargetIdentified {
        ticket,
        result: Ok(target(ScrollTargetFrame {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 40.0,
        })),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollWindowsPrepared {
            ticket,
            result: Err(ScrollWindowError::MappingFailed)
        }),
        vec![
            CaptureEffect::ScrollRefused(
                ticket,
                ScrollRefusal::Windows(ScrollWindowError::MappingFailed)
            ),
            CaptureEffect::PresentFrozen(ticket)
        ]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert!(!session.output_busy());
}

#[test]
fn core_session_36_successful_preparation_fixes_the_exclusion_set() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    session.dispatch(CaptureAction::ScrollTargetIdentified {
        ticket,
        result: Ok(target(ScrollTargetFrame {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 40.0,
        })),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollWindowsPrepared {
            ticket,
            result: Ok(vec![11, 12])
        }),
        vec![CaptureEffect::ScrollPrepared(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Prepared);
    assert_eq!(session.scroll_exclusion(), Some(&[11, 12][..]));
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_37_cancel_scroll_keeps_the_region_selection() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    let selection = session.document().unwrap().selection().unwrap();
    assert_eq!(
        session.dispatch(CaptureAction::CancelScroll { ticket }),
        vec![CaptureEffect::PresentFrozen(ticket)]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
    assert_eq!(session.document().unwrap().selection(), Some(selection));
}

#[test]
fn core_session_38_uncertain_native_state_is_terminal_and_forbids_restart() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollNativeUncertain { ticket }),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ScrollRelaunchRequired
        ]
    );
    assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    // No output, no partial result and no in-session restart.
    assert_eq!(
        session.dispatch(CaptureAction::Start),
        vec![CaptureEffect::ReportFailure(
            CaptureFailure::NativeUncertain
        )]
    );
    assert_eq!(session.phase(), CapturePhase::RelaunchRequired);
    assert!(!session.may_present(ticket));
}

#[test]
fn core_session_39_display_loss_cancels_the_whole_session() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    assert_eq!(
        session.dispatch(CaptureAction::DisplayInvalidated),
        vec![
            CaptureEffect::DismissOverlay,
            CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid)
        ]
    );
    assert_eq!(session.phase(), CapturePhase::Resident);
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert!(session.frame().is_none());
    assert!(!session.may_present(ticket));
}

#[test]
fn core_session_40_hotkey_restart_abandons_scroll_preparation() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    let effects = session.dispatch(CaptureAction::Start);
    assert!(effects.contains(&CaptureEffect::DismissOverlay));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CaptureEffect::CheckPermission(_)))
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::CheckingPermission);
}

#[test]
fn core_session_41_stale_scroll_actions_never_affect_another_attempt() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    session.dispatch(CaptureAction::CancelScroll { ticket });
    // A late result from the cancelled preparation changes nothing.
    assert_eq!(
        session.dispatch(CaptureAction::ScrollWindowsPrepared {
            ticket,
            result: Ok(vec![1])
        }),
        vec![]
    );
    assert_eq!(session.scroll_phase(), ScrollPhase::Inactive);
    assert_eq!(session.phase(), CapturePhase::Frozen);
}

#[test]
fn core_session_42_selection_outside_a_zero_extent_window_is_geometry() {
    let (mut session, ticket) = frozen_selected();
    enter(&mut session, ticket);
    session.dispatch(CaptureAction::ScrollPermissionsChecked {
        ticket,
        available: permissions(true, true, true),
    });
    assert_eq!(
        session.dispatch(CaptureAction::ScrollTargetIdentified {
            ticket,
            result: Ok(target(ScrollTargetFrame {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0
            }))
        }),
        vec![
            CaptureEffect::ScrollRefused(
                ticket,
                ScrollRefusal::Target(ScrollTargetError::Geometry)
            ),
            CaptureEffect::PresentFrozen(ticket)
        ]
    );
}

/// The three-permission matrix is user-facing: each entry has a stable key, a
/// distinct label and a purpose that names what the permission is used for.
#[test]
fn core_session_43_scroll_permission_matrix_is_specific_and_content_free() {
    use arcshot_core::ScrollPermission;
    let all = ScrollPermission::ALL;
    assert_eq!(all.len(), 3);
    for permission in all {
        assert!(!permission.key().is_empty());
        assert!(!permission.label().is_empty());
        let purpose = permission.purpose();
        assert!(!purpose.is_empty());
        // The purpose names the permission's use, not a TCC denial reason.
        assert!(!purpose.contains("拒绝"));
        assert!(!purpose.contains("未知"));
    }
    let keys: Vec<_> = all.iter().map(|permission| permission.key()).collect();
    assert_eq!(
        keys,
        ["screen_recording", "input_monitoring", "accessibility"]
    );
    let missing = ScrollPermissions {
        screen_recording: true,
        input_monitoring: false,
        accessibility: false,
    }
    .missing();
    assert_eq!(
        missing,
        vec![
            ScrollPermission::InputMonitoring,
            ScrollPermission::Accessibility
        ]
    );
    assert!(
        ScrollPermissions {
            screen_recording: true,
            input_monitoring: true,
            accessibility: true
        }
        .all_available()
    );
}
