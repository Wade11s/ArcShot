//! Capture side-effect scheduling at the app's externally testable boundary.
mod capture_tasks;
pub use capture_tasks::{CaptureSubmitError, CaptureTasks};
mod permission_flow;
pub use permission_flow::{
    GuideHistory, PermissionAction, PermissionEffect, PermissionFlow, PermissionNotice,
};
mod hotkey_flow;
pub use hotkey_flow::{HotkeyAction, HotkeyEffect, HotkeyFlow, HotkeyNotice, HotkeyOutcome};
mod preferences_flow;
pub use preferences_flow::{
    DensityWriteOutcome, LoginItemOutcome, PreferencesAction, PreferencesEffect, PreferencesFlow,
    PreferencesNotice,
};
mod output_tasks;
pub use output_tasks::{OutputSubmitError, OutputTasks};
mod output_publish;
pub use output_publish::{PendingSave, encode_document, publish_output};
mod key_policy;
pub use key_policy::{
    CaptureKeyAction, CaptureKeyContext, CapturePressAction, KeyModifiers, capture_key_action,
    capture_press_action, capture_text_press_action,
};
