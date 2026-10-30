//! APP-EFFECT: hotkey settings policy drives native registration and persistence.
use arcshot_app::{
    HotkeyAction as A, HotkeyEffect as E, HotkeyFlow, HotkeyNotice as N, HotkeyOutcome as O,
};
use arcshot_core::Hotkey;

fn alternate() -> Hotkey {
    Hotkey::new('c', false, true, false, false)
}

#[test]
fn app_effect_04_launch_registers_persisted_hotkey_without_storing() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    assert_eq!(flow.config(), Hotkey::default());
    assert_eq!(
        flow.dispatch(A::Launch),
        vec![E::Register(Hotkey::default())]
    );
    assert_eq!(flow.dispatch(A::RegisterResult(O::Applied)), vec![]);
    assert_eq!(flow.registered(), Some(Hotkey::default()));
}

#[test]
fn app_effect_05_startup_registration_failure_prompts_and_keeps_config() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    assert_eq!(
        flow.dispatch(A::RegisterResult(O::Failed)),
        vec![E::Notice(N::StartupRegistrationFailed)]
    );
    assert_eq!(flow.config(), Hotkey::default());
    assert_eq!(flow.registered(), None);
}

#[test]
fn app_effect_06_successful_save_replaces_and_persists() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    flow.dispatch(A::RegisterResult(O::Applied));
    assert_eq!(
        flow.dispatch(A::Save(alternate())),
        vec![E::Register(alternate())]
    );
    assert_eq!(
        flow.dispatch(A::RegisterResult(O::Applied)),
        vec![E::Store(alternate())]
    );
    assert_eq!(flow.config(), alternate());
    assert_eq!(flow.registered(), Some(alternate()));
}

#[test]
fn app_effect_07_rejected_save_keeps_old_config_and_restores_suspended_registration() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    flow.dispatch(A::RegisterResult(O::Applied));
    // The recording area activates: the active hotkey is suspended first.
    assert_eq!(flow.dispatch(A::RecordingStarted), vec![E::Suspend]);
    assert_eq!(flow.dispatch(A::SuspendResult(O::Applied)), vec![]);
    assert_eq!(
        flow.dispatch(A::Save(alternate())),
        vec![E::Register(alternate())]
    );
    assert_eq!(
        flow.dispatch(A::RegisterResult(O::Failed)),
        vec![E::Notice(N::SaveRejected), E::Resume]
    );
    assert_eq!(flow.config(), Hotkey::default());
    assert_eq!(flow.dispatch(A::ResumeResult(O::Applied)), vec![]);
    assert_eq!(flow.registered(), Some(Hotkey::default()));
}

#[test]
fn app_effect_08_restore_failure_reports_hotkey_not_active() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    flow.dispatch(A::RegisterResult(O::Applied));
    flow.dispatch(A::RecordingStarted);
    flow.dispatch(A::SuspendResult(O::Applied));
    flow.dispatch(A::Save(alternate()));
    flow.dispatch(A::RegisterResult(O::Failed));
    assert_eq!(
        flow.dispatch(A::ResumeResult(O::Failed)),
        vec![E::Notice(N::RestoreFailed)]
    );
    // The old configuration is kept but must not be displayed as active.
    assert_eq!(flow.config(), Hotkey::default());
    assert_eq!(flow.registered(), None);
}

#[test]
fn app_effect_09_failed_suspension_aborts_recording_and_keeps_registration() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    flow.dispatch(A::RegisterResult(O::Applied));
    flow.dispatch(A::RecordingStarted);
    assert_eq!(
        flow.dispatch(A::SuspendResult(O::Failed)),
        vec![E::Notice(N::SuspendFailed)]
    );
    assert_eq!(flow.registered(), Some(Hotkey::default()));
    // Recording ended afterwards must not issue a resume.
    assert_eq!(flow.dispatch(A::RecordingEnded), vec![]);
}

#[test]
fn app_effect_10_escape_ends_recording_and_resumes_without_saving() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    flow.dispatch(A::RegisterResult(O::Applied));
    flow.dispatch(A::RecordingStarted);
    flow.dispatch(A::SuspendResult(O::Applied));
    assert_eq!(flow.dispatch(A::RecordingEnded), vec![E::Resume]);
    assert_eq!(flow.dispatch(A::ResumeResult(O::Applied)), vec![]);
    assert_eq!(flow.registered(), Some(Hotkey::default()));
    assert_eq!(flow.config(), Hotkey::default());
    // Saving the unchanged combination does nothing.
    assert_eq!(flow.dispatch(A::Save(Hotkey::default())), vec![]);
}

#[test]
fn app_effect_11_saving_while_recording_replaces_suspension_without_restore() {
    let mut flow = HotkeyFlow::new(Hotkey::default());
    flow.dispatch(A::Launch);
    flow.dispatch(A::RegisterResult(O::Applied));
    flow.dispatch(A::RecordingStarted);
    flow.dispatch(A::SuspendResult(O::Applied));
    // The old registration stays suspended; a successful save must not also
    // resume it, and a failed save must restore it exactly once.
    assert_eq!(
        flow.dispatch(A::Save(alternate())),
        vec![E::Register(alternate())]
    );
    assert_eq!(
        flow.dispatch(A::RegisterResult(O::Applied)),
        vec![E::Store(alternate())]
    );
    assert_eq!(flow.config(), alternate());
    assert_eq!(flow.registered(), Some(alternate()));
    flow.dispatch(A::RecordingStarted);
    flow.dispatch(A::SuspendResult(O::Applied));
    flow.dispatch(A::Save(Hotkey::new('d', true, false, false, false)));
    assert_eq!(
        flow.dispatch(A::RegisterResult(O::Failed)),
        vec![E::Notice(N::SaveRejected), E::Resume]
    );
    assert_eq!(flow.dispatch(A::ResumeResult(O::Applied)), vec![]);
    assert_eq!(flow.registered(), Some(alternate()));
    assert_eq!(flow.config(), alternate());
}
