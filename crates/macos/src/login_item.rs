//! Launch-at-login through the system's main-app login item.
//!
//! The adapter only exposes the ServiceManagement status and the register /
//! unregister request. It never remembers a requested value: callers re-read
//! [`login_item_status`] after every change, so an intent can never be displayed
//! as an enabled login item.

use std::{error::Error, fmt};

use arcshot_core::LoginItemState;
use objc2_foundation::NSError;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// Reads the actual login-item state reported by ServiceManagement.
///
/// This is a query only: it neither registers the app nor opens System Settings.
#[must_use]
pub fn login_item_status() -> LoginItemState {
    // SAFETY: `mainAppService` is a class method with no arguments; `status` only
    // reads the service's current state and does not start or stop anything.
    let service = unsafe { SMAppService::mainAppService() };
    login_item_state(unsafe { service.status() })
}

fn login_item_state(status: SMAppServiceStatus) -> LoginItemState {
    if status == SMAppServiceStatus::Enabled {
        LoginItemState::Enabled
    } else if status == SMAppServiceStatus::RequiresApproval {
        LoginItemState::RequiresApproval
    } else {
        // `NotRegistered`, `NotFound`, and any future status cannot launch the app.
        LoginItemState::Disabled
    }
}

/// Asks ServiceManagement to register or unregister this app as a login item.
///
/// Success only means the request was accepted. The caller must read
/// [`login_item_status`] before telling the user that the change took effect.
///
/// # Errors
///
/// Returns [`LoginItemError`] when the system rejects the request, for example
/// because the bundle is not code signed or registration was denied by the user.
pub fn set_login_item_enabled(enabled: bool) -> Result<(), LoginItemError> {
    // SAFETY: Both calls target the main-app service and only change this app's
    // own login-item registration; the returned error is retained by the binding.
    let service = unsafe { SMAppService::mainAppService() };
    let result = if enabled {
        unsafe { service.registerAndReturnError() }
    } else {
        unsafe { service.unregisterAndReturnError() }
    };
    result.map_err(LoginItemError::from)
}

/// A login-item request the system rejected or could not complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginItemError {
    message: String,
}

impl From<objc2::rc::Retained<NSError>> for LoginItemError {
    fn from(error: objc2::rc::Retained<NSError>) -> Self {
        Self {
            message: format!(
                "{} ({}, {})",
                error.localizedDescription(),
                error.domain(),
                error.code()
            ),
        }
    }
}

impl fmt::Display for LoginItemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LoginItemError {}
