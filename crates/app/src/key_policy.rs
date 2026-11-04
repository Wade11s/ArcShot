//! Overlay key priority and pointer gesture routing before semantic dispatch.
//!
//! The view keeps no document state of its own beyond the transient band and
//! drag flag; these functions only decide which command the overlay queues.
use arcshot_core::Selection;

/// Modifier set the overlay shortcuts distinguish. Command is macOS's platform key.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    /// Command (the platform modifier on macOS).
    pub command: bool,
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// Option.
    pub option: bool,
}

/// What the overlay should queue for a key press, if anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureKeyAction {
    /// Esc: cancel the session without output.
    Cancel,
    /// Enter: finish the current valid selection.
    Finish,
    /// Command+Z: undo the most recent document edit.
    Undo,
    /// Command+Shift+Z: redo the most recently undone edit.
    Redo,
}

/// What a primary-button press on the overlay should start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CapturePressAction {
    /// Double-click: the core finishes only when the point is inside the selection.
    FinishAt([f64; 2]),
    /// Press inside the selection or on a control point: the core owns the drag.
    Drag,
    /// Press outside: the overlay shows a new-selection band committed on release.
    Band,
}

/// Applies I-09 priority for keys plus the I-08 undo/redo shortcuts.
///
/// While the mouse button is held, keys are ignored so one drag stays one
/// undoable step. Enter also waits for output/save processing, and undo/redo are
/// editing, so they wait too. Esc stays usable during output; its late result is
/// fenced by the document version.
#[must_use]
pub fn capture_key_action(
    key: &str,
    modifiers: KeyModifiers,
    busy: bool,
    dragging: bool,
) -> Option<CaptureKeyAction> {
    let command_only = modifiers.command && !modifiers.control && !modifiers.option;
    match key {
        "escape" if !dragging => Some(CaptureKeyAction::Cancel),
        "enter" if !busy && !dragging => Some(CaptureKeyAction::Finish),
        "z" if command_only && !modifiers.shift && !busy && !dragging => {
            Some(CaptureKeyAction::Undo)
        }
        "z" if command_only && modifiers.shift && !busy && !dragging => {
            Some(CaptureKeyAction::Redo)
        }
        _ => None,
    }
}

/// Routes one primary-button press; the core still validates the selection target.
///
/// A control point keeps its resize gesture even on a double-click, while a
/// double-click on the selection blank is Finish (I-09 priority).
#[must_use]
pub fn capture_press_action(
    double_click: bool,
    busy: bool,
    selection: Option<Selection>,
    point: [f64; 2],
) -> Option<CapturePressAction> {
    if busy {
        return None;
    }
    if let Some(selection) = selection
        && selection.resize_handle_at(point).is_some()
    {
        return Some(CapturePressAction::Drag);
    }
    if double_click {
        return Some(CapturePressAction::FinishAt(point));
    }
    match selection {
        Some(selection) if selection.contains(point) => Some(CapturePressAction::Drag),
        _ => Some(CapturePressAction::Band),
    }
}
