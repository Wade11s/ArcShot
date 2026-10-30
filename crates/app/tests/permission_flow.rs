//! APP-EFFECT: preference and permission results, without TCC interaction.
use arcshot_app::{
    GuideHistory, PermissionAction as A, PermissionEffect as E, PermissionFlow, PermissionNotice,
};
#[test]
fn app_effect_01_skipped_guide_is_not_automatically_repeated_after_restart() {
    let mut flow = PermissionFlow::new(GuideHistory::Unseen);
    assert!(flow.dispatch(A::Launch).is_empty());
    assert_eq!(flow.notice(), Some(PermissionNotice::FirstUse));
    assert_eq!(flow.dispatch(A::Later), vec![E::RememberGuide]);
    assert_eq!(flow.notice(), None);
    let mut restarted = PermissionFlow::new(GuideHistory::Handled);
    assert!(restarted.dispatch(A::Launch).is_empty());
    assert_eq!(restarted.notice(), None);
    assert_eq!(restarted.dispatch(A::Check), vec![E::Preflight]);
}
#[test]
fn app_effect_02_settings_return_rechecks_actual_availability_without_starting_capture() {
    let mut flow = PermissionFlow::new(GuideHistory::Unseen);
    flow.dispatch(A::Launch);
    assert_eq!(
        flow.dispatch(A::Configure),
        vec![E::RememberGuide, E::RequestScreenRecording, E::OpenSettings]
    );
    assert_eq!(flow.dispatch(A::ReturnedFromSettings), vec![E::Preflight]);
    flow.dispatch(A::Checked(false));
    assert_eq!(flow.notice(), Some(PermissionNotice::Unavailable));
    assert_eq!(flow.dispatch(A::Recheck), vec![E::Preflight]);
    flow.dispatch(A::Checked(true));
    assert_eq!(flow.notice(), Some(PermissionNotice::Available));
}
#[test]
fn app_effect_03_capture_unavailable_offers_settings_recheck_and_cancel() {
    let mut flow = PermissionFlow::new(GuideHistory::Handled);
    flow.dispatch(A::Unavailable);
    assert_eq!(flow.notice(), Some(PermissionNotice::Unavailable));
    assert_eq!(
        flow.dispatch(A::Configure),
        vec![E::RememberGuide, E::RequestScreenRecording, E::OpenSettings]
    );
    flow.dispatch(A::Cancel);
    assert_eq!(flow.notice(), None);
    assert!(flow.dispatch(A::ReturnedFromSettings).is_empty());
}
