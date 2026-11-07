//! APP-EFFECT: Scroll-only permission notice policy with injected results.
use arcshot_app::{
    ScrollPermissionAction as A, ScrollPermissionEffect as E, ScrollPermissionFlow,
    ScrollPermissionNotice,
};
use arcshot_core::{ScrollPermission, ScrollPermissions};

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

#[test]
fn app_effect_38_missing_permissions_offer_settings_recheck_and_cancel() {
    let mut flow = ScrollPermissionFlow::default();
    let missing = vec![
        ScrollPermission::InputMonitoring,
        ScrollPermission::Accessibility,
    ];
    assert_eq!(
        flow.dispatch(A::Checked(permissions(true, false, false))),
        vec![E::ShowNotice(ScrollPermissionNotice::Missing(
            missing.clone()
        ))]
    );
    assert_eq!(
        flow.notice(),
        Some(&ScrollPermissionNotice::Missing(missing))
    );
    assert_eq!(
        flow.dispatch(A::OpenSettings(ScrollPermission::InputMonitoring)),
        vec![E::OpenSettings(ScrollPermission::InputMonitoring)]
    );
    assert!(flow.waiting_for_settings());
    // Returning from Settings re-reads the real preflight; it never starts Scroll.
    assert_eq!(flow.dispatch(A::ReturnedFromSettings), vec![E::Preflight]);
    assert_eq!(
        flow.dispatch(A::Checked(permissions(true, true, true))),
        vec![E::ShowNotice(ScrollPermissionNotice::Ready)]
    );
    assert_eq!(flow.dispatch(A::Cancel), vec![]);
    assert_eq!(flow.notice(), None);
    assert!(!flow.waiting_for_settings());
}

#[test]
fn app_effect_39_recheck_never_auto_starts_scroll() {
    let mut flow = ScrollPermissionFlow::default();
    flow.dispatch(A::Checked(permissions(true, false, false)));
    assert_eq!(flow.dispatch(A::Recheck), vec![E::Preflight]);
    // Even a fully available re-check only reports Ready; the user must re-enter.
    assert_eq!(
        flow.dispatch(A::Checked(permissions(true, true, true))),
        vec![E::ShowNotice(ScrollPermissionNotice::Ready)]
    );
    assert_eq!(flow.notice(), Some(&ScrollPermissionNotice::Ready));
}

#[test]
fn app_effect_40_cancel_clears_the_notice_and_settings_wait() {
    let mut flow = ScrollPermissionFlow::default();
    flow.dispatch(A::Checked(permissions(false, false, false)));
    flow.dispatch(A::OpenSettings(ScrollPermission::ScreenRecording));
    assert!(flow.waiting_for_settings());
    assert_eq!(flow.dispatch(A::Cancel), vec![]);
    assert_eq!(flow.notice(), None);
    assert!(!flow.waiting_for_settings());
    // A stale return after cancel changes nothing.
    assert_eq!(flow.dispatch(A::ReturnedFromSettings), vec![]);
}

#[test]
fn app_effect_41_ready_notice_does_not_emit_a_start_effect() {
    let mut flow = ScrollPermissionFlow::default();
    let effects = flow.dispatch(A::Checked(permissions(true, true, true)));
    assert_eq!(effects, vec![E::ShowNotice(ScrollPermissionNotice::Ready)]);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, E::Preflight | E::OpenSettings(_)))
    );
}
