//! APP-EFFECT: the I-04 menu preferences with injected store and native outcomes.
use arcshot_app::{
    DensityWriteOutcome, LoginItemOutcome, PreferencesAction as A, PreferencesEffect as E,
    PreferencesFlow, PreferencesNotice,
};
use arcshot_core::{Density, LoginItemState};

fn launched(completion_sound: bool, login_item: LoginItemState) -> PreferencesFlow {
    let mut flow = PreferencesFlow::new(completion_sound, Density::Compact);
    assert_eq!(flow.dispatch(A::Launch), vec![E::ReadLoginItemStatus]);
    assert_eq!(
        flow.dispatch(A::LoginItemStatusRead(Some(login_item))),
        vec![]
    );
    flow
}

#[test]
fn app_effect_27_completion_sound_defaults_on_and_every_toggle_persists_the_new_value() {
    let mut flow = PreferencesFlow::new(true, Density::Compact);
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
    let mut restored = PreferencesFlow::new(false, Density::Compact);
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
    let mut flow = PreferencesFlow::new(true, Density::Compact);
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
    let mut flow = PreferencesFlow::new(true, Density::Compact);
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

// --- I-09 选项密度 (APP-EFFECT-33..37) -------------------------------------

#[test]
fn app_effect_33_density_defaults_compact_and_a_restart_restores_the_injected_value() {
    let flow = PreferencesFlow::new(true, Density::Compact);
    assert_eq!(flow.density(), Density::Compact, "I-09 default is Compact");
    // A restart injects the real persisted value; no default is re-applied.
    let mut restored = PreferencesFlow::new(true, Density::Standard);
    assert_eq!(restored.density(), Density::Standard);
    // Launch only reads the login item; it neither writes nor reflects density.
    assert_eq!(restored.dispatch(A::Launch), vec![E::ReadLoginItemStatus]);
}

#[test]
fn app_effect_34_toggle_density_derives_intent_from_a_fresh_read_and_persists_it() {
    let mut flow = PreferencesFlow::new(true, Density::Compact);
    assert_eq!(flow.dispatch(A::ToggleDensity), vec![E::ReadDensity]);
    // One change at a time: a second click while the read is outstanding queues
    // no second intent.
    assert_eq!(flow.dispatch(A::ToggleDensity), vec![]);
    // A fresh read that disagrees with the injected value wins (real state).
    assert_eq!(
        flow.dispatch(A::DensityRead(Some(Density::Standard))),
        vec![E::StoreDensity(Density::Compact)]
    );
    // A write in flight is also guarded.
    assert_eq!(flow.dispatch(A::ToggleDensity), vec![]);
    assert_eq!(
        flow.dispatch(A::DensityWriteResult(DensityWriteOutcome::Applied)),
        vec![E::ReflectDensity(Density::Compact)],
        "only a confirmed write changes the shown density"
    );
    assert_eq!(flow.density(), Density::Compact);

    // A second toggle flips from the now-current value.
    assert_eq!(flow.dispatch(A::ToggleDensity), vec![E::ReadDensity]);
    assert_eq!(
        flow.dispatch(A::DensityRead(Some(Density::Compact))),
        vec![E::StoreDensity(Density::Standard)]
    );
    assert_eq!(
        flow.dispatch(A::DensityWriteResult(DensityWriteOutcome::Applied)),
        vec![E::ReflectDensity(Density::Standard)]
    );
    assert_eq!(flow.density(), Density::Standard);
}

#[test]
fn app_effect_35_unreadable_density_reports_instead_of_silently_changing_intent() {
    let mut flow = PreferencesFlow::new(true, Density::Compact);
    assert_eq!(flow.dispatch(A::ToggleDensity), vec![E::ReadDensity]);
    assert_eq!(
        flow.dispatch(A::DensityRead(None)),
        vec![E::Notice(PreferencesNotice::DensityUnreadable)],
        "a failed read never silently flips the user's choice"
    );
    assert_eq!(flow.density(), Density::Compact);
    assert_eq!(flow.notice(), Some(PreferencesNotice::DensityUnreadable));

    // The notice is dismissible and the menu path stays available for a retry.
    assert_eq!(flow.dispatch(A::DismissNotice), vec![]);
    assert_eq!(flow.notice(), None);
    assert_eq!(flow.dispatch(A::ToggleDensity), vec![E::ReadDensity]);
    assert_eq!(
        flow.dispatch(A::DensityRead(Some(Density::Compact))),
        vec![E::StoreDensity(Density::Standard)]
    );
}

#[test]
fn app_effect_36_failed_density_write_rereads_and_never_shows_the_intent_as_applied() {
    let mut flow = PreferencesFlow::new(true, Density::Compact);
    flow.dispatch(A::ToggleDensity);
    assert_eq!(
        flow.dispatch(A::DensityRead(Some(Density::Compact))),
        vec![E::StoreDensity(Density::Standard)]
    );
    assert_eq!(
        flow.dispatch(A::DensityWriteResult(DensityWriteOutcome::Failed)),
        vec![
            E::Notice(PreferencesNotice::DensityWriteFailed),
            E::ReadDensity,
        ],
        "a rejected write reports and re-reads the real state"
    );
    assert_eq!(
        flow.density(),
        Density::Compact,
        "the intent is not reflected"
    );
    assert_eq!(flow.notice(), Some(PreferencesNotice::DensityWriteFailed));
    // The verification read only syncs the real value; it emits no reflection.
    assert_eq!(
        flow.dispatch(A::DensityRead(Some(Density::Compact))),
        vec![]
    );
    assert_eq!(flow.density(), Density::Compact);

    // A verification read that also fails reports that, still without a write.
    let mut flow = PreferencesFlow::new(true, Density::Compact);
    flow.dispatch(A::ToggleDensity);
    flow.dispatch(A::DensityRead(Some(Density::Compact)));
    flow.dispatch(A::DensityWriteResult(DensityWriteOutcome::Failed));
    assert_eq!(
        flow.dispatch(A::DensityRead(None)),
        vec![E::Notice(PreferencesNotice::DensityUnreadable)]
    );
    assert_eq!(flow.density(), Density::Compact);
    assert_eq!(flow.notice(), Some(PreferencesNotice::DensityUnreadable));
}

#[test]
fn app_effect_37_a_density_switch_never_emits_another_preferences_effect() {
    // Switching density must preserve the completion-sound and login-item state:
    // the policy produces only density effects, never a sound or login effect.
    let mut flow = PreferencesFlow::new(true, Density::Compact);
    let effects = [
        flow.dispatch(A::ToggleDensity),
        flow.dispatch(A::DensityRead(Some(Density::Compact))),
        flow.dispatch(A::DensityWriteResult(DensityWriteOutcome::Applied)),
    ]
    .concat();
    assert!(effects.iter().all(|effect| matches!(
        effect,
        E::ReadDensity | E::StoreDensity(_) | E::ReflectDensity(_)
    )));
    assert!(flow.completion_sound());
    assert_eq!(flow.login_item(), None);
}
