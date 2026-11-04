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
mod scroll_flow;
pub use scroll_flow::{
    ScrollPermissionAction, ScrollPermissionEffect, ScrollPermissionFlow, ScrollPermissionNotice,
};
mod scroll_target_tasks;
pub use scroll_target_tasks::{ScrollTargetSubmitError, ScrollTargetTasks};
mod scroll_frame_tasks;
pub use scroll_frame_tasks::{
    MAX_RAW_FRAMES, ScrollFrameQueue, ScrollFrameQueueError, ScrollFrameSubmitError,
    ScrollFrameTasks, admit_scroll_capture, may_submit_scroll_frame,
};
mod scroll_input_flow;
pub use scroll_input_flow::{ScrollTapCommand, scroll_tap_action};
mod output_publish;
pub use output_publish::{PendingSave, encode_document, publish_output};
mod completion_preview;
pub use completion_preview::{MARGIN, MAX_HEIGHT, MAX_WIDTH, PreviewLayout, preview_layout};
mod key_policy;
pub use key_policy::{
    CaptureKeyAction, CaptureKeyContext, CapturePressAction, KeyModifiers, capture_key_action,
    capture_long_image_press_action, capture_press_action, capture_text_press_action,
};
