//! APP-KEY: I-09 key priority and overlay pointer gesture routing.
use arcshot_app::{
    CaptureKeyAction, CapturePressAction, KeyModifiers, capture_key_action, capture_press_action,
};
use arcshot_core::{ResizeHandle, Selection};

fn selection() -> Selection {
    Selection::try_new(10.0, 5.0, 30.0, 20.0).unwrap()
}

fn command() -> KeyModifiers {
    KeyModifiers {
        command: true,
        ..Default::default()
    }
}

fn command_shift() -> KeyModifiers {
    KeyModifiers {
        command: true,
        shift: true,
        ..Default::default()
    }
}

#[test]
fn app_key_01_escape_and_enter_are_ignored_while_the_mouse_is_held() {
    assert_eq!(
        capture_key_action("escape", KeyModifiers::default(), false, true),
        None
    );
    assert_eq!(
        capture_key_action("enter", KeyModifiers::default(), false, true),
        None
    );
}

#[test]
fn app_key_02_enter_waits_for_output_and_escape_still_cancels() {
    assert_eq!(
        capture_key_action("enter", KeyModifiers::default(), true, false),
        None
    );
    assert_eq!(
        capture_key_action("escape", KeyModifiers::default(), true, false),
        Some(CaptureKeyAction::Cancel)
    );
}

#[test]
fn app_key_03_idle_keys_map_to_finish_and_cancel_only() {
    assert_eq!(
        capture_key_action("enter", KeyModifiers::default(), false, false),
        Some(CaptureKeyAction::Finish)
    );
    assert_eq!(
        capture_key_action("escape", KeyModifiers::default(), false, false),
        Some(CaptureKeyAction::Cancel)
    );
    assert_eq!(
        capture_key_action("a", KeyModifiers::default(), false, false),
        None
    );
}

#[test]
fn app_key_04_command_z_undoes_and_command_shift_z_redoes_only() {
    assert_eq!(
        capture_key_action("z", command(), false, false),
        Some(CaptureKeyAction::Undo)
    );
    assert_eq!(
        capture_key_action("z", command_shift(), false, false),
        Some(CaptureKeyAction::Redo)
    );
    assert_eq!(
        capture_key_action("z", KeyModifiers::default(), false, false),
        None
    );
    assert_eq!(
        capture_key_action(
            "z",
            KeyModifiers {
                command: true,
                option: true,
                ..Default::default()
            },
            false,
            false
        ),
        None
    );
    assert_eq!(
        capture_key_action(
            "z",
            KeyModifiers {
                command: true,
                control: true,
                ..Default::default()
            },
            false,
            false
        ),
        None
    );
}

#[test]
fn app_key_05_undo_and_redo_wait_for_output_and_released_drags() {
    assert_eq!(capture_key_action("z", command(), true, false), None);
    assert_eq!(capture_key_action("z", command_shift(), true, false), None);
    assert_eq!(capture_key_action("z", command(), false, true), None);
    assert_eq!(capture_key_action("z", command_shift(), false, true), None);
}

#[test]
fn app_key_06_press_inside_the_selection_or_on_a_handle_starts_the_core_drag() {
    let selection = selection();
    assert_eq!(
        capture_press_action(false, false, Some(selection), [20.0, 10.0]),
        Some(CapturePressAction::Drag)
    );
    assert_eq!(
        capture_press_action(
            false,
            false,
            Some(selection),
            selection.handle_center(ResizeHandle::TopLeft)
        ),
        Some(CapturePressAction::Drag)
    );
}

#[test]
fn app_key_07_press_outside_or_without_a_selection_starts_a_new_band() {
    let selection = selection();
    assert_eq!(
        capture_press_action(false, false, Some(selection), [5.0, 5.0]),
        Some(CapturePressAction::Band)
    );
    assert_eq!(
        capture_press_action(false, false, None, [5.0, 5.0]),
        Some(CapturePressAction::Band)
    );
}

#[test]
fn app_key_08_double_click_finishes_at_the_point_and_busy_ignores_every_press() {
    let selection = selection();
    assert_eq!(
        capture_press_action(true, false, Some(selection), [20.0, 10.0]),
        Some(CapturePressAction::FinishAt([20.0, 10.0]))
    );
    // Outside the selection the core sees the point and refuses Finish.
    assert_eq!(
        capture_press_action(true, false, Some(selection), [90.0, 45.0]),
        Some(CapturePressAction::FinishAt([90.0, 45.0]))
    );
    // A control point keeps its own resize gesture even on a double-click.
    assert_eq!(
        capture_press_action(
            true,
            false,
            Some(selection),
            selection.handle_center(ResizeHandle::BottomRight)
        ),
        Some(CapturePressAction::Drag)
    );
    assert_eq!(
        capture_press_action(false, true, Some(selection), [20.0, 10.0]),
        None
    );
    assert_eq!(
        capture_press_action(true, true, Some(selection), [20.0, 10.0]),
        None
    );
}
