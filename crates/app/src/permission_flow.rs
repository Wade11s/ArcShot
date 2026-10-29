//! Screen Recording guide policy with explicit preference and native effects.

/// User-visible permission notices, not inferred TCC denial reasons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionNotice {
    /// First-launch explanation with Configure and Later.
    FirstUse,
    /// Preflight currently reports usable Screen Recording.
    Available,
    /// Preflight is unavailable; offer Settings, Recheck and Cancel.
    Unavailable,
}
/// Semantic permission actions at the app/system boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionAction {
    /// Application launched with the persisted guide preference.
    Launch,
    /// User opened the permission menu.
    Check,
    /// A native preflight completed.
    Checked(bool),
    /// Capture preflight failed; show recovery without another prompt.
    Unavailable,
    /// User explicitly chose Configure or Open Settings.
    Configure,
    /// User deferred the first guide.
    Later,
    /// User dismissed the permission notice.
    Cancel,
    /// User manually rechecks availability.
    Recheck,
    /// The app became active again after visiting Settings.
    ReturnedFromSettings,
}
/// Concrete side effects; there is no scroll-specific permission requirement here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionEffect {
    /// Persist the fact that the first guide was handled.
    RememberGuide,
    /// Read actual Screen Recording availability.
    Preflight,
    /// User-authorized first system Screen Recording request.
    RequestScreenRecording,
    /// Open the Screen Recording settings pane.
    OpenSettings,
}
/// The persisted first-guide choice, separate from permission availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuideHistory {
    /// The first guide has not been handled.
    Unseen,
    /// The user configured or deferred the guide.
    Handled,
}

/// Permission notice policy; the persisted preference is injected at startup.
#[derive(Debug)]
pub struct PermissionFlow {
    seen: bool,
    notice: Option<PermissionNotice>,
    waiting_for_settings: bool,
}
impl PermissionFlow {
    /// Restores the persisted guide decision without doing system I/O.
    #[must_use]
    pub const fn new(history: GuideHistory) -> Self {
        Self {
            seen: matches!(history, GuideHistory::Handled),
            notice: None,
            waiting_for_settings: false,
        }
    }
    /// Returns the current notice, if any.
    #[must_use]
    pub const fn notice(&self) -> Option<PermissionNotice> {
        self.notice
    }
    /// Applies user intent or a real native result; never guesses a denial reason.
    pub fn dispatch(&mut self, action: PermissionAction) -> Vec<PermissionEffect> {
        use PermissionAction as A;
        use PermissionEffect as E;
        match action {
            A::Launch => {
                if !self.seen {
                    self.notice = Some(PermissionNotice::FirstUse);
                }
                vec![]
            }
            A::Check | A::Recheck => vec![E::Preflight],
            A::Checked(available) => {
                self.notice = Some(if available {
                    PermissionNotice::Available
                } else {
                    PermissionNotice::Unavailable
                });
                vec![]
            }
            A::Unavailable => {
                self.notice = Some(PermissionNotice::Unavailable);
                vec![]
            }
            A::Configure => {
                self.seen = true;
                self.waiting_for_settings = true;
                self.notice = Some(PermissionNotice::Unavailable);
                // Also register a permission request after the user deferred the
                // first guide; otherwise Settings may not list ArcShot at all.
                vec![E::RememberGuide, E::RequestScreenRecording, E::OpenSettings]
            }
            A::Later | A::Cancel => {
                let remember = self.notice == Some(PermissionNotice::FirstUse);
                self.seen |= remember;
                self.notice = None;
                self.waiting_for_settings = false;
                if remember {
                    vec![E::RememberGuide]
                } else {
                    vec![]
                }
            }
            A::ReturnedFromSettings => {
                if self.waiting_for_settings {
                    self.waiting_for_settings = false;
                    vec![E::Preflight]
                } else {
                    vec![]
                }
            }
        }
    }
}
