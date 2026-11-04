//! APP-EFFECT: the two I-04 menu preferences with injected store and native outcomes.
use arcshot_app::{
    LoginItemOutcome, PreferencesAction as A, PreferencesEffect as E, PreferencesFlow,
    PreferencesNotice,
};
use arcshot_core::LoginItemState;

fn launched(completion_sound: bool, login_item: LoginItemState) -> PreferencesFlow {
    let mut flow = PreferencesFlow::new(completion_sound);
    assert_eq!(flow.dispatch(A::Launch), vec![E::ReadLoginItemStatus]);
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(login_item))),
        vec![]
    );
    flow
}

#[test]
fn app_effect_27_completion_sound_defaults_on_and_every_toggle_persists_the_new_value() {
    let mut flow = PreferencesFlow::new(true);
    assert!(flow.completion_sound(), "I-04 default is on");
    assert_eq!(
        flow.dispatch(A::ToggleCompletionSound),
        vec![E::StoreCompletionSound(false)],
        "turning the sound off is persisted before the next output"
    );
    assert!(!flow.completion_sound());
    assert_eq!(
        flow.dispatch(A::ToggleCompletionSound),
        vec![E::StoreCompletionSound(true)]
    );
    assert!(flow.completion_sound());

    // The persisted value, not the default, is what a restart restores.
    let mut restored = PreferencesFlow::new(false);
    assert!(!restored.completion_sound());
    assert_eq!(restored.dispatch(A::Launch), vec![E::ReadLoginItemStatus]);
}

#[test]
fn app_effect_28_login_item_intent_always_comes_from_a_fresh_actual_read() {
    // The menu-open read reports the real disabled state, not the default.
    let mut flow = launched(false, LoginItemState::Disabled);
    assert_eq!(flow.login_item(), Some(LoginItemState::Disabled));
    assert_eq!(
        flow.dispatch(A::ToggleLoginItem),
        vec![E::ReadLoginItemStatus]
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled))),
        vec![E::SetLoginItem(true)],
        "turning it on is requested only after the actual off state was read"
    );

    // A later request derives the intent from what the system says now, even
    // when the app last saw a different value.
    let mut flow = launched(false, LoginItemState::Disabled);
    assert_eq!(
        flow.dispatch(A::ToggleLoginItem),
        vec![E::ReadLoginItemStatus]
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Enabled))),
        vec![E::SetLoginItem(false)],
        "a menu-open read that disagrees with the last app value wins"
    );
    assert_eq!(flow.login_item(), Some(LoginItemState::Enabled));
}

#[test]
fn app_effect_29_applied_change_is_verified_by_a_real_read_not_by_the_intent() {
    let mut flow = launched(false, LoginItemState::Disabled);
    flow.dispatch(A::ToggleLoginItem);
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled))),
        vec![E::SetLoginItem(true)]
    );
    assert_eq!(
        flow.dispatch(A::LoginItemSetResult(LoginItemOutcome::Applied)),
        vec![E::ReadLoginItemStatus],
        "success is never assumed from the set call alone"
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Enabled))),
        vec![],
        "the verified state matches the intent, so nothing is reported"
    );
    assert_eq!(flow.login_item(), Some(LoginItemState::Enabled));
    assert_eq!(flow.notice(), None);

    // The same applied call with a read that still reports off is a failure.
    let mut flow = launched(false, LoginItemState::Disabled);
    flow.dispatch(A::ToggleLoginItem);
    flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled)));
    assert_eq!(
        flow.dispatch(A::LoginItemSetResult(LoginItemOutcome::Applied)),
        vec![E::ReadLoginItemStatus]
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled))),
        vec![E::Notice(PreferencesNotice::LoginItemToggleFailed)],
        "the actual state is reported instead of showing the intent as applied"
    );
    assert_eq!(flow.login_item(), Some(LoginItemState::Disabled));
    assert_eq!(
        flow.notice(),
        Some(PreferencesNotice::LoginItemToggleFailed)
    );
}

#[test]
fn app_effect_30_failed_toggle_rereads_the_actual_state_and_can_be_retried_after_dismissal() {
    let mut flow = launched(false, LoginItemState::Disabled);
    flow.dispatch(A::ToggleLoginItem);
    flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled)));
    assert_eq!(
        flow.dispatch(A::LoginItemSetResult(LoginItemOutcome::Failed)),
        vec![
            E::Notice(PreferencesNotice::LoginItemToggleFailed),
            E::ReadLoginItemStatus,
        ],
        "a rejected change re-reads the real status before it is reported"
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled))),
        vec![]
    );
    assert_eq!(flow.login_item(), Some(LoginItemState::Disabled));
    assert_eq!(
        flow.notice(),
        Some(PreferencesNotice::LoginItemToggleFailed)
    );

    // The notice is dismissible; the menu path stays available for a retry.
    assert_eq!(flow.dispatch(A::DismissNotice), vec![]);
    assert_eq!(flow.notice(), None);
    assert_eq!(
        flow.dispatch(A::ToggleLoginItem),
        vec![E::ReadLoginItemStatus]
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled))),
        vec![E::SetLoginItem(true)]
    );
}

#[test]
fn app_effect_31_unreadable_login_item_state_never_requests_a_blind_change() {
    // A read failure on the toggle path blocks the change and reports it.
    let mut flow = PreferencesFlow::new(true);
    flow.dispatch(A::Launch);
    assert_eq!(
        flow.dispatch(A::ToggleLoginItem),
        vec![E::ReadLoginItemStatus]
    );
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(None)),
        vec![E::Notice(PreferencesNotice::LoginItemUnreadable)]
    );
    assert_eq!(flow.login_item(), None);
    assert_eq!(flow.notice(), Some(PreferencesNotice::LoginItemUnreadable));

    // A failed verification read after a set also reports instead of assuming.
    let mut flow = launched(false, LoginItemState::Disabled);
    flow.dispatch(A::ToggleLoginItem);
    flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled)));
    flow.dispatch(A::LoginItemSetResult(LoginItemOutcome::Applied));
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(None)),
        vec![E::Notice(PreferencesNotice::LoginItemUnreadable)]
    );
    assert_eq!(flow.login_item(), None);

    // A launch-time read failure is silent: no menu action was attempted.
    let mut flow = PreferencesFlow::new(true);
    flow.dispatch(A::Launch);
    assert_eq!(flow.dispatch(A::LoginItemStatusRead(None)), vec![]);
    assert_eq!(flow.notice(), None);
}

#[test]
fn app_effect_32_requires_approval_is_reported_instead_of_showing_the_intent() {
    let mut flow = launched(false, LoginItemState::Disabled);
    flow.dispatch(A::ToggleLoginItem);
    flow.dispatch(A::LoginItemStatusRead(Some(LoginItemState::Disabled)));
    flow.dispatch(A::LoginItemSetResult(LoginItemOutcome::Applied));
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(
            LoginItemState::RequiresApproval
        ))),
        vec![E::Notice(PreferencesNotice::LoginItemApprovalRequired)],
        "a registered item awaiting approval is not a working login item"
    );
    assert_eq!(
        flow.login_item(),
        Some(LoginItemState::RequiresApproval),
        "the reported state is the system's, so the menu shows it as not enabled"
    );
}
