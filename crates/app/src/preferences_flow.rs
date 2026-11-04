//! Menu preference policy: completion sound and the login item, with injected results.
//!
//! Mirrors `permission_flow` and `hotkey_flow`: the coordinator injects the persisted
//! completion-sound value, the actual login-item reads and the native set outcomes,
//! then executes the returned effects against the macOS adapter. This type performs
//! no persistence, no login-item query and no I/O of its own.

use arcshot_core::LoginItemState;

/// Result of the native request to enable or disable the login item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginItemOutcome {
    /// The system accepted the request; the actual state is still unverified.
    Applied,
    /// The system rejected or failed the request.
    Failed,
}

/// User-visible preference failures; the coordinator supplies the text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferencesNotice {
    /// The login item could not be enabled or disabled; its actual state was re-read.
    LoginItemToggleFailed,
    /// No actual login-item state could be read, so no change was requested.
    LoginItemUnreadable,
    /// The login item is registered but waits for approval in System Settings.
    LoginItemApprovalRequired,
}

/// Semantic preference actions at the app/native boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferencesAction {
    /// Application launched; read the login item without reporting failures.
    Launch,
    /// The user toggled 完成音效 in the status menu.
    ToggleCompletionSound,
    /// The user toggled 登录时启动; a fresh actual read decides the intent.
    ToggleLoginItem,
    /// Result of an outstanding login-item status read; `None` means unreadable.
    LoginItemStatusRead(Option<LoginItemState>),
    /// Result of the outstanding login-item enable/disable request.
    LoginItemSetResult(LoginItemOutcome),
    /// The user dismissed the preference failure notice.
    DismissNotice,
}

/// Native and persistent side effects guarded by this policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferencesEffect {
    /// Persist the completion-sound preference locally.
    StoreCompletionSound(bool),
    /// Read the actual login-item status from the system.
    ReadLoginItemStatus,
    /// Ask the system to enable or disable the login item.
    SetLoginItem(bool),
    /// Surface a preference failure in the in-product notice window.
    Notice(PreferencesNotice),
}

/// How the actual login-item state relates to the requested one.
fn satisfied(intent: bool, state: LoginItemState) -> bool {
    match (intent, state) {
        (true, LoginItemState::Enabled) | (false, LoginItemState::Disabled) => true,
        // A registered item that still waits for approval is neither active nor
        // unregistered, so it satisfies neither intent.
        _ => false,
    }
}

/// Menu preference policy; the persisted sound value is injected at startup.
#[derive(Debug)]
pub struct PreferencesFlow {
    completion_sound: bool,
    login_item: Option<LoginItemState>,
    toggle_requested: bool,
    pending: Option<bool>,
    verifying: Option<bool>,
    notice: Option<PreferencesNotice>,
}

impl PreferencesFlow {
    /// Restores the persisted completion-sound value without any system I/O.
    #[must_use]
    pub const fn new(completion_sound: bool) -> Self {
        Self {
            completion_sound,
            login_item: None,
            toggle_requested: false,
            pending: None,
            verifying: None,
            notice: None,
        }
    }

    /// Whether a successful output plays the short completion sound.
    #[must_use]
    pub const fn completion_sound(&self) -> bool {
        self.completion_sound
    }

    /// The last actual login-item state the system reported, if one was readable.
    #[must_use]
    pub const fn login_item(&self) -> Option<LoginItemState> {
        self.login_item
    }

    /// The failure notice that still needs an in-product window, if any.
    #[must_use]
    pub const fn notice(&self) -> Option<PreferencesNotice> {
        self.notice
    }

    /// Applies user intent or an injected native result.
    ///
    /// Every login-item change is decided by a fresh read and verified by another
    /// read, so a requested state is never displayed as if it had taken effect.
    pub fn dispatch(&mut self, action: PreferencesAction) -> Vec<PreferencesEffect> {
        use PreferencesAction as A;
        use PreferencesEffect as E;
        match action {
            A::Launch => vec![E::ReadLoginItemStatus],
            A::ToggleCompletionSound => {
                self.completion_sound = !self.completion_sound;
                vec![E::StoreCompletionSound(self.completion_sound)]
            }
            A::ToggleLoginItem => {
                // One change at a time; a repeated click while a read or set is
                // outstanding must not queue a second intent.
                if self.toggle_requested || self.pending.is_some() || self.verifying.is_some() {
                    return vec![];
                }
                self.notice = None;
                self.toggle_requested = true;
                vec![E::ReadLoginItemStatus]
            }
            A::LoginItemStatusRead(read) => {
                self.login_item = read;
                if self.toggle_requested {
                    self.toggle_requested = false;
                    let Some(state) = read else {
                        self.notice = Some(PreferencesNotice::LoginItemUnreadable);
                        return vec![E::Notice(PreferencesNotice::LoginItemUnreadable)];
                    };
                    let intent = state != LoginItemState::Enabled;
                    self.pending = Some(intent);
                    return vec![E::SetLoginItem(intent)];
                }
                let Some(intent) = self.verifying.take() else {
                    return vec![]; // A menu-open or launch read only refreshes the state.
                };
                let Some(state) = read else {
                    self.notice = Some(PreferencesNotice::LoginItemUnreadable);
                    return vec![E::Notice(PreferencesNotice::LoginItemUnreadable)];
                };
                if satisfied(intent, state) {
                    return vec![];
                }
                let notice = if intent && state == LoginItemState::RequiresApproval {
                    PreferencesNotice::LoginItemApprovalRequired
                } else {
                    PreferencesNotice::LoginItemToggleFailed
                };
                self.notice = Some(notice);
                vec![E::Notice(notice)]
            }
            A::LoginItemSetResult(outcome) => {
                let Some(intent) = self.pending.take() else {
                    return vec![];
                };
                match outcome {
                    LoginItemOutcome::Applied => {
                        // The request is confirmed by a fresh read, never by itself.
                        self.verifying = Some(intent);
                        vec![E::ReadLoginItemStatus]
                    }
                    LoginItemOutcome::Failed => {
                        self.notice = Some(PreferencesNotice::LoginItemToggleFailed);
                        vec![
                            E::Notice(PreferencesNotice::LoginItemToggleFailed),
                            E::ReadLoginItemStatus,
                        ]
                    }
                }
            }
            A::DismissNotice => {
                self.notice = None;
                vec![]
            }
        }
    }
}
