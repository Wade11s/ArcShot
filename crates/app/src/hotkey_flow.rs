//! Global capture hotkey settings policy with explicit native effects.
//!
//! Pure state machine mirroring `permission_flow`: the coordinator injects
//! native registration results and executes the returned effects against the
//! macOS adapter. It never performs registration, persistence or I/O itself.

use arcshot_core::Hotkey;

/// Outcome of a native hotkey operation, injected back by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyOutcome {
    /// The native adapter applied the operation.
    Applied,
    /// The native adapter refused or failed the operation.
    Failed,
}

/// User-visible hotkey failure notices; the coordinator supplies the text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyNotice {
    /// Launch registration failed; the hotkey is not active; the menu remains usable.
    StartupRegistrationFailed,
    /// The candidate could not be registered; the old configuration is kept.
    SaveRejected,
    /// The suspended hotkey could not be re-registered; it is NOT active now.
    RestoreFailed,
    /// The active hotkey could not be suspended; recording was aborted.
    SuspendFailed,
}

/// Semantic hotkey actions at the app/native boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyAction {
    /// Application launched; register the persisted or default hotkey.
    Launch,
    /// Result of the outstanding [`HotkeyEffect::Register`].
    RegisterResult(HotkeyOutcome),
    /// The recording area was activated; suspend the active registration.
    RecordingStarted,
    /// Result of the outstanding [`HotkeyEffect::Suspend`].
    SuspendResult(HotkeyOutcome),
    /// Recording ended (Esc, candidate captured or window closed).
    RecordingEnded,
    /// Result of the outstanding [`HotkeyEffect::Resume`].
    ResumeResult(HotkeyOutcome),
    /// User asked to save a recorded candidate.
    Save(Hotkey),
    /// The settings window closed; resume any suspended registration.
    SettingsClosed,
}

/// Native and persistent side effects guarded by this policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyEffect {
    /// Make this combination the single active registration.
    Register(Hotkey),
    /// Unregister the active hotkey, remembering its value.
    Suspend,
    /// Re-register the remembered suspended value.
    Resume,
    /// Persist the configuration locally.
    Store(Hotkey),
    /// Surface a failure notice.
    Notice(HotkeyNotice),
}

/// Hotkey settings policy; the persisted configuration is injected at startup.
#[derive(Debug)]
pub struct HotkeyFlow {
    config: Hotkey,
    registered: Option<Hotkey>,
    suspended: Option<Hotkey>,
    pending_save: Option<Hotkey>,
}

impl HotkeyFlow {
    /// Restores the persisted (or default) configuration without system I/O.
    #[must_use]
    pub const fn new(config: Hotkey) -> Self {
        Self {
            config,
            registered: None,
            suspended: None,
            pending_save: None,
        }
    }

    /// The configured hotkey, regardless of registration success.
    #[must_use]
    pub const fn config(&self) -> Hotkey {
        self.config
    }

    /// The combination actually registered natively, if any.
    #[must_use]
    pub const fn registered(&self) -> Option<Hotkey> {
        self.registered
    }

    /// Applies user intent or injected native results.
    pub fn dispatch(&mut self, action: HotkeyAction) -> Vec<HotkeyEffect> {
        match action {
            HotkeyAction::Launch => vec![HotkeyEffect::Register(self.config)],
            HotkeyAction::RegisterResult(outcome) => match (outcome, self.pending_save.take()) {
                (HotkeyOutcome::Applied, Some(candidate)) => {
                    self.config = candidate;
                    self.registered = Some(candidate);
                    self.suspended = None;
                    vec![HotkeyEffect::Store(candidate)]
                }
                (HotkeyOutcome::Applied, None) => {
                    self.registered = Some(self.config);
                    vec![]
                }
                (HotkeyOutcome::Failed, Some(_)) => {
                    let mut effects = vec![HotkeyEffect::Notice(HotkeyNotice::SaveRejected)];
                    if self.suspended.is_some() {
                        // Best effort: bring the untouched old registration back.
                        effects.push(HotkeyEffect::Resume);
                    }
                    effects
                }
                (HotkeyOutcome::Failed, None) => {
                    vec![HotkeyEffect::Notice(
                        HotkeyNotice::StartupRegistrationFailed,
                    )]
                }
            },
            HotkeyAction::RecordingStarted => match self.registered.take() {
                Some(active) => {
                    self.suspended = Some(active);
                    vec![HotkeyEffect::Suspend]
                }
                None => vec![],
            },
            HotkeyAction::SuspendResult(HotkeyOutcome::Applied) => vec![],
            HotkeyAction::SuspendResult(HotkeyOutcome::Failed) => {
                self.registered = self.suspended.take();
                vec![HotkeyEffect::Notice(HotkeyNotice::SuspendFailed)]
            }
            HotkeyAction::RecordingEnded | HotkeyAction::SettingsClosed => {
                if self.suspended.is_some() {
                    vec![HotkeyEffect::Resume]
                } else {
                    vec![]
                }
            }
            HotkeyAction::ResumeResult(HotkeyOutcome::Applied) => {
                self.registered = self.suspended.take();
                vec![]
            }
            HotkeyAction::ResumeResult(HotkeyOutcome::Failed) => {
                vec![HotkeyEffect::Notice(HotkeyNotice::RestoreFailed)]
            }
            HotkeyAction::Save(candidate) => {
                if candidate == self.config && self.pending_save.is_none() {
                    return vec![];
                }
                self.pending_save = Some(candidate);
                vec![HotkeyEffect::Register(candidate)]
            }
        }
    }
}
