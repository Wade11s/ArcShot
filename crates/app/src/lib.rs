//! Capture side-effect scheduling at the app's externally testable boundary.
mod capture_tasks;
pub use capture_tasks::{CaptureSubmitError, CaptureTasks};
mod permission_flow;
pub use permission_flow::{
    GuideHistory, PermissionAction, PermissionEffect, PermissionFlow, PermissionNotice,
};
mod hotkey_flow;
pub use hotkey_flow::{HotkeyAction, HotkeyEffect, HotkeyFlow, HotkeyNotice, HotkeyOutcome};
mod output_tasks;
pub use output_tasks::{OutputSubmitError, OutputTasks};
mod key_policy;
pub use key_policy::{CaptureKeyAction, capture_key_action};
