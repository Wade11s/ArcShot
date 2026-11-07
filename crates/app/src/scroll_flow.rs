//! Scroll-only permission notice policy with injected native results.
//!
//! The authoritative decision to enter, continue or refuse Scroll preparation
//! lives in `arcshot_core::session`. This flow only owns the user-facing notice:
//! which missing permissions to explain, which settings pane to open, and the
//! rule that a successful re-check never auto-starts Scroll.

use arcshot_core::{ScrollPermission, ScrollPermissions};

/// The Scroll permission notice the coordinator renders.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScrollPermissionNotice {
    /// One or more permissions are still missing.
    Missing(Vec<ScrollPermission>),
    /// Every permission is available; the user must start Scroll again manually.
    Ready,
}

/// User intent or a real native result for the Scroll permission notice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollPermissionAction {
    /// A native preflight completed.
    Checked(ScrollPermissions),
    /// Open the matching System Settings pane.
    OpenSettings(ScrollPermission),
    /// The app became active again after visiting Settings.
    ReturnedFromSettings,
    /// The user manually rechecks.
    Recheck,
    /// Dismiss the notice.
    Cancel,
}

/// Concrete side effects; the flow never opens Settings or reads TCC itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScrollPermissionEffect {
    /// Read the real three-permission preflight again.
    Preflight,
    /// Open one permission's settings pane.
    OpenSettings(ScrollPermission),
    /// Show or update the notice.
    ShowNotice(ScrollPermissionNotice),
}

/// Scroll permission notice policy; no system handles or clocks.
#[derive(Debug, Default)]
pub struct ScrollPermissionFlow {
    notice: Option<ScrollPermissionNotice>,
    waiting_for_settings: bool,
}

impl ScrollPermissionFlow {
    /// The current notice, if any.
    #[must_use]
    pub const fn notice(&self) -> Option<&ScrollPermissionNotice> {
        self.notice.as_ref()
    }

    /// Whether the app is waiting for the user to return from Settings.
    #[must_use]
    pub const fn waiting_for_settings(&self) -> bool {
        self.waiting_for_settings
    }

    /// Applies one action and returns the effects the coordinator must run.
    pub fn dispatch(&mut self, action: ScrollPermissionAction) -> Vec<ScrollPermissionEffect> {
        match action {
            ScrollPermissionAction::Checked(permissions) => {
                let notice = if permissions.all_available() {
                    ScrollPermissionNotice::Ready
                } else {
                    ScrollPermissionNotice::Missing(permissions.missing())
                };
                self.notice = Some(notice.clone());
                vec![ScrollPermissionEffect::ShowNotice(notice)]
            }
            ScrollPermissionAction::OpenSettings(permission) => {
                self.waiting_for_settings = true;
                vec![ScrollPermissionEffect::OpenSettings(permission)]
            }
            ScrollPermissionAction::ReturnedFromSettings => {
                if self.waiting_for_settings {
                    self.waiting_for_settings = false;
                    vec![ScrollPermissionEffect::Preflight]
                } else {
                    vec![]
                }
            }
            ScrollPermissionAction::Recheck => vec![ScrollPermissionEffect::Preflight],
            ScrollPermissionAction::Cancel => {
                self.notice = None;
                self.waiting_for_settings = false;
                vec![]
            }
        }
    }
}
