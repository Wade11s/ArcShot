//! APP-KEY: I-09 key priority and overlay pointer gesture routing.
use arcshot_app::{
    CaptureKeyAction, CaptureKeyContext, CapturePressAction, KeyModifiers, capture_key_action,
    capture_press_action,
};
use arcshot_core::{ResizeHandle, Selection, Tool};

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
        capture_key_action("escape", KeyModifiers::default(), context(false, true)),
        None
    );
    assert_eq!(
        capture_key_action("enter", KeyModifiers::default(), context(false, true)),
        None
    );
}

#[test]
fn app_key_02_enter_waits_for_output_and_escape_still_cancels() {
    assert_eq!(
        capture_key_action("enter", KeyModifiers::default(), context(true, false)),
        None
    );
    assert_eq!(
        capture_key_action("escape", KeyModifiers::default(), context(true, false)),
        Some(CaptureKeyAction::Cancel)
    );
}

#[test]
fn app_key_03_idle_keys_map_to_finish_and_cancel_only() {
    assert_eq!(
        capture_key_action("enter", KeyModifiers::default(), context(false, false)),
        Some(CaptureKeyAction::Finish)
    );
    assert_eq!(
        capture_key_action("escape", KeyModifiers::default(), context(false, false)),
        Some(CaptureKeyAction::Cancel)
    );
    assert_eq!(
        capture_key_action("a", KeyModifiers::default(), context(false, false)),
        None
    );
}

#[test]
fn app_key_04_command_z_undoes_and_command_shift_z_redoes_only() {
    assert_eq!(
        capture_key_action("z", command(), context(false, false)),
        Some(CaptureKeyAction::Undo)
    );
    assert_eq!(
        capture_key_action("z", command_shift(), context(false, false)),
        Some(CaptureKeyAction::Redo)
    );
    assert_eq!(
        capture_key_action("z", KeyModifiers::default(), context(false, false)),
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
            context(false, false)
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
            context(false, false)
        ),
        None
    );
}

#[test]
fn app_key_05_undo_and_redo_wait_for_output_and_released_drags() {
    assert_eq!(
        capture_key_action("z", command(), context(true, false)),
        None
    );
    assert_eq!(
        capture_key_action("z", command_shift(), context(true, false)),
        None
    );
    assert_eq!(
        capture_key_action("z", command(), context(false, true)),
        None
    );
    assert_eq!(
        capture_key_action("z", command_shift(), context(false, true)),
        None
    );
}

#[test]
fn app_key_06_press_inside_the_selection_or_on_a_handle_starts_the_core_drag() {
    let selection = selection();
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::Drag)
    );
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            false,
            selection.handle_center(ResizeHandle::TopLeft)
        ),
        Some(CapturePressAction::Drag)
    );
}

#[test]
fn app_key_07_press_outside_or_without_a_selection_starts_a_new_band() {
    let selection = selection();
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            false,
            [5.0, 5.0]
        ),
        Some(CapturePressAction::Band)
    );
    assert_eq!(
        capture_press_action(false, false, Tool::Select, None, false, [5.0, 5.0]),
        Some(CapturePressAction::Band)
    );
}

#[test]
fn app_key_08_double_click_finishes_at_the_point_and_busy_ignores_every_press() {
    let selection = selection();
    assert_eq!(
        capture_press_action(
            true,
            false,
            Tool::Select,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::FinishAt([20.0, 10.0]))
    );
    // Outside the selection the core sees the point and refuses Finish.
    assert_eq!(
        capture_press_action(
            true,
            false,
            Tool::Select,
            Some(selection),
            false,
            [90.0, 45.0]
        ),
        Some(CapturePressAction::FinishAt([90.0, 45.0]))
    );
    // A control point keeps its own resize gesture even on a double-click.
    assert_eq!(
        capture_press_action(
            true,
            false,
            Tool::Select,
            Some(selection),
            false,
            selection.handle_center(ResizeHandle::BottomRight)
        ),
        Some(CapturePressAction::Drag)
    );
    assert_eq!(
        capture_press_action(
            false,
            true,
            Tool::Select,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        None
    );
    assert_eq!(
        capture_press_action(
            true,
            true,
            Tool::Select,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        None
    );
}

fn context(busy: bool, dragging: bool) -> CaptureKeyContext {
    CaptureKeyContext {
        busy,
        dragging,
        ..Default::default()
    }
}

fn with_object(context: CaptureKeyContext, tool: Tool) -> CaptureKeyContext {
    CaptureKeyContext {
        object_selected: true,
        tool,
        ..context
    }
}

#[test]
fn app_key_09_escape_layers_object_tool_then_session() {
    // A selected object is cleared first.
    assert_eq!(
        capture_key_action(
            "escape",
            KeyModifiers::default(),
            with_object(context(false, false), Tool::Rectangle)
        ),
        Some(CaptureKeyAction::Deselect)
    );
    // Otherwise an active annotation tool returns to the select tool.
    assert_eq!(
        capture_key_action(
            "escape",
            KeyModifiers::default(),
            CaptureKeyContext {
                tool: Tool::Rectangle,
                ..Default::default()
            }
        ),
        Some(CaptureKeyAction::SelectTool)
    );
    // Otherwise the session is cancelled.
    assert_eq!(
        capture_key_action("escape", KeyModifiers::default(), context(false, false)),
        Some(CaptureKeyAction::Cancel)
    );
    // A held drag ignores Esc; output still lets Esc cancel the session.
    assert_eq!(
        capture_key_action(
            "escape",
            KeyModifiers::default(),
            with_object(context(false, true), Tool::Rectangle)
        ),
        None
    );
    assert_eq!(
        capture_key_action(
            "escape",
            KeyModifiers::default(),
            with_object(context(true, false), Tool::Rectangle)
        ),
        Some(CaptureKeyAction::Cancel)
    );
}

#[test]
fn app_key_10_delete_and_backspace_remove_only_a_selected_object() {
    for key in ["delete", "backspace"] {
        assert_eq!(
            capture_key_action(key, KeyModifiers::default(), context(false, false)),
            None,
            "{key}"
        );
        assert_eq!(
            capture_key_action(
                key,
                KeyModifiers::default(),
                CaptureKeyContext {
                    object_selected: true,
                    ..Default::default()
                }
            ),
            Some(CaptureKeyAction::DeleteObject),
            "{key}"
        );
        assert_eq!(
            capture_key_action(key, KeyModifiers::default(), context(true, false)),
            None,
            "{key}"
        );
        assert_eq!(
            capture_key_action(
                key,
                KeyModifiers::default(),
                with_object(context(false, true), Tool::Select)
            ),
            None,
            "{key}"
        );
    }
    // Modified delete keys stay with the platform.
    assert_eq!(
        capture_key_action(
            "backspace",
            command(),
            with_object(context(false, false), Tool::Select)
        ),
        None
    );
}

#[test]
fn app_key_11_an_active_annotation_tool_creates_inside_and_never_finishes_on_double_click() {
    let selection = selection();
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Rectangle,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::Create)
    );
    // Holding the tool, a double-click inside is a creation, not Finish.
    assert_eq!(
        capture_press_action(
            true,
            false,
            Tool::Rectangle,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::Create)
    );
    // Creation cannot start outside; the gesture is a new selection band.
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Rectangle,
            Some(selection),
            false,
            [5.0, 5.0]
        ),
        Some(CapturePressAction::Band)
    );
    // Enter still finishes, but Esc leaves the tool first.
    assert_eq!(
        capture_key_action(
            "enter",
            KeyModifiers::default(),
            CaptureKeyContext {
                tool: Tool::Rectangle,
                ..Default::default()
            }
        ),
        Some(CaptureKeyAction::Finish)
    );
}

#[test]
fn app_key_12_object_hits_win_over_every_selection_gesture() {
    let selection = selection();
    // A body hit inside the selection beats a selection move.
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            true,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::ObjectDrag)
    );
    // An object hit wins even on a selection control point.
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            true,
            selection.handle_center(ResizeHandle::TopLeft)
        ),
        Some(CapturePressAction::ObjectDrag)
    );
    // A double-click on an object is an object gesture, not Finish.
    assert_eq!(
        capture_press_action(
            true,
            false,
            Tool::Select,
            Some(selection),
            true,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::ObjectDrag)
    );
    // Without an object hit the selection keeps its own gestures.
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            false,
            selection.handle_center(ResizeHandle::BottomRight)
        ),
        Some(CapturePressAction::Drag)
    );
    assert_eq!(
        capture_press_action(
            false,
            false,
            Tool::Select,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::Drag)
    );
    assert_eq!(
        capture_press_action(
            true,
            false,
            Tool::Select,
            Some(selection),
            false,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::FinishAt([20.0, 10.0]))
    );
    // Busy output ignores every press, including object gestures.
    assert_eq!(
        capture_press_action(
            true,
            true,
            Tool::Select,
            Some(selection),
            true,
            [20.0, 10.0]
        ),
        None
    );
}

fn press(
    double_click: bool,
    busy: bool,
    tool: Tool,
    selection: Option<Selection>,
    object_hit: bool,
    point: [f64; 2],
) -> Option<CapturePressAction> {
    capture_press_action(double_click, busy, tool, selection, object_hit, point)
}

#[test]
fn app_key_13_line_and_arrow_tools_create_inside_and_never_finish_on_double_click() {
    let selection = selection();
    for tool in [Tool::Line, Tool::Arrow] {
        // A press inside creates; a press outside starts a new selection band.
        assert_eq!(
            press(false, false, tool, Some(selection), false, [20.0, 10.0]),
            Some(CapturePressAction::Create),
            "{tool:?}"
        );
        assert_eq!(
            press(false, false, tool, Some(selection), false, [5.0, 5.0]),
            Some(CapturePressAction::Band),
            "{tool:?}"
        );
        // A double-click never finishes while an annotation tool is active.
        assert_eq!(
            press(true, false, tool, Some(selection), false, [20.0, 10.0]),
            Some(CapturePressAction::Create),
            "{tool:?}"
        );
        assert_eq!(
            press(true, false, tool, Some(selection), false, [5.0, 5.0]),
            Some(CapturePressAction::Band),
            "{tool:?}"
        );
        // An object hit is still a new object gesture, not an object drag.
        assert_eq!(
            press(false, false, tool, Some(selection), true, [20.0, 10.0]),
            Some(CapturePressAction::Create),
            "{tool:?}"
        );
        // Without a selection there is no creation area.
        assert_eq!(
            press(false, false, tool, None, false, [20.0, 10.0]),
            Some(CapturePressAction::Band),
            "{tool:?}"
        );
        // Output processing ignores the press.
        assert_eq!(
            press(false, true, tool, Some(selection), false, [20.0, 10.0]),
            None,
            "{tool:?}"
        );
        // Enter still finishes and Esc leaves the tool layer first.
        let context = CaptureKeyContext {
            tool,
            ..Default::default()
        };
        assert_eq!(
            capture_key_action("enter", KeyModifiers::default(), context),
            Some(CaptureKeyAction::Finish),
            "{tool:?}"
        );
        assert_eq!(
            capture_key_action("escape", KeyModifiers::default(), context),
            Some(CaptureKeyAction::SelectTool),
            "{tool:?}"
        );
    }
}

#[test]
fn app_key_14_escape_layers_line_and_arrow_objects_then_tools_then_session() {
    for tool in [Tool::Line, Tool::Arrow] {
        assert_eq!(
            capture_key_action(
                "escape",
                KeyModifiers::default(),
                with_object(context(false, false), tool)
            ),
            Some(CaptureKeyAction::Deselect),
            "{tool:?}"
        );
        assert_eq!(
            capture_key_action(
                "escape",
                KeyModifiers::default(),
                CaptureKeyContext {
                    tool,
                    ..context(false, false)
                }
            ),
            Some(CaptureKeyAction::SelectTool),
            "{tool:?}"
        );
        assert_eq!(
            capture_key_action(
                "escape",
                KeyModifiers::default(),
                CaptureKeyContext {
                    tool,
                    dragging: true,
                    ..Default::default()
                }
            ),
            None,
            "{tool:?}"
        );
        assert_eq!(
            capture_key_action(
                "escape",
                KeyModifiers::default(),
                CaptureKeyContext {
                    tool,
                    busy: true,
                    ..Default::default()
                }
            ),
            Some(CaptureKeyAction::Cancel),
            "{tool:?}"
        );
        // Delete still needs a selected object even with a line tool active.
        assert_eq!(
            capture_key_action(
                "backspace",
                KeyModifiers::default(),
                CaptureKeyContext {
                    tool,
                    ..Default::default()
                }
            ),
            None,
            "{tool:?}"
        );
    }
}

#[test]
fn app_key_15_a_line_object_hit_is_an_object_gesture_only_with_the_select_tool() {
    let selection = selection();
    // The select tool routes a line body/endpoint hit to the core object drag.
    assert_eq!(
        press(
            true,
            false,
            Tool::Select,
            Some(selection),
            true,
            [20.0, 10.0]
        ),
        Some(CapturePressAction::ObjectDrag)
    );
    // An active line or arrow tool keeps creating above that same object.
    for tool in [Tool::Line, Tool::Arrow] {
        assert_eq!(
            press(true, false, tool, Some(selection), true, [20.0, 10.0]),
            Some(CapturePressAction::Create),
            "{tool:?}"
        );
    }
}
