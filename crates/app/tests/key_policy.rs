//! APP-KEY: I-09 key priority for drag and output gestures.
use arcshot_app::{CaptureKeyAction, capture_key_action};

#[test]
fn app_key_01_escape_and_enter_are_ignored_while_the_mouse_is_held() {
    assert_eq!(capture_key_action("escape", false, true), None);
    assert_eq!(capture_key_action("enter", false, true), None);
}

#[test]
fn app_key_02_enter_waits_for_output_and_escape_still_cancels() {
    assert_eq!(capture_key_action("enter", true, false), None);
    assert_eq!(
        capture_key_action("escape", true, false),
        Some(CaptureKeyAction::Cancel)
    );
}

#[test]
fn app_key_03_idle_keys_map_to_finish_and_cancel_only() {
    assert_eq!(
        capture_key_action("enter", false, false),
        Some(CaptureKeyAction::Finish)
    );
    assert_eq!(
        capture_key_action("escape", false, false),
        Some(CaptureKeyAction::Cancel)
    );
    assert_eq!(capture_key_action("a", false, false), None);
}
