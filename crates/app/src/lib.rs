//! Capture side-effect scheduling at the app's externally testable boundary.
mod capture_tasks;
pub use capture_tasks::{CaptureSubmitError, CaptureTasks};
mod permission_flow;
pub use permission_flow::{
    GuideHistory, PermissionAction, PermissionEffect, PermissionFlow, PermissionNotice,
};
