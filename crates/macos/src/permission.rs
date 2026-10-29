//! Screen Recording permission and the persistent first-run guide flag.

use std::{error::Error, fmt};

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL, NSUserDefaults};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

const GUIDE_SEEN_KEY: &str = "ScreenRecordingPermissionGuideSeen";
const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

/// Checks Screen Recording access without requesting it or capturing a screen.
#[must_use]
pub fn screen_recording_available() -> bool {
    // SAFETY: This system function has no arguments and does not require a main thread.
    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Requests Screen Recording access. The return value is the system's current answer;
/// callers should preflight again after returning from settings.
#[must_use]
pub fn request_screen_recording() -> bool {
    // SAFETY: This system function has no arguments and does not expose borrowed data.
    unsafe { CGRequestScreenCaptureAccess() }
}

/// Failure to open the Screen Recording pane in System Settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermissionError;

impl fmt::Display for PermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not open screen recording settings")
    }
}

impl Error for PermissionError {}

/// Opens the Screen Recording pane; does not request any other permission.
///
/// # Errors
/// Returns an error if the URL cannot be constructed or the system refuses to open it.
pub fn open_screen_recording_settings() -> Result<(), PermissionError> {
    let url = NSURL::URLWithString(&NSString::from_str(SETTINGS_URL)).ok_or(PermissionError)?;
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(PermissionError)
    }
}

/// Whether the user has already been shown the first-run permission guide.
#[must_use]
pub fn permission_guide_seen() -> bool {
    NSUserDefaults::standardUserDefaults().boolForKey(&NSString::from_str(GUIDE_SEEN_KEY))
}

/// Persists that the first-run guide was shown, regardless of permission outcome.
pub fn mark_permission_guide_seen() {
    NSUserDefaults::standardUserDefaults()
        .setBool_forKey(true, &NSString::from_str(GUIDE_SEEN_KEY));
}
