//! Overlay key priority and pointer gesture routing before semantic dispatch.
//!
//! The view keeps no document state of its own beyond the transient band and
//! drag flag; these functions only decide which command the overlay queues and
//! the authoritative document validates every one of them.
use arcshot_core::{Selection, Tool};

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

/// Document facts the overlay shortcuts need, mirrored from the authority.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaptureKeyContext {
    /// Output processing or a save dialog owns the document.
    pub busy: bool,
    /// A pointer drag is held; keys wait for release.
    pub dragging: bool,
    /// The document currently has a single selected object.
    pub object_selected: bool,
    /// The active pointer tool.
    pub tool: Tool,
}

/// What the overlay should queue for a key press, if anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureKeyAction {
    /// Esc with no object/tool layer left: cancel the session without output.
    Cancel,
    /// Enter: finish the current valid selection.
    Finish,
    /// Command+Z: undo the most recent document edit.
    Undo,
    /// Command+Shift+Z: redo the most recently undone edit.
    Redo,
    /// Esc layer one: clear the selected object.
    Deselect,
    /// Esc layer two: leave the active annotation tool for the select tool.
    SelectTool,
    /// Delete/Backspace: delete the selected object.
    DeleteObject,
}

/// What a primary-button press on the overlay should start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CapturePressAction {
    /// Double-click: the core finishes only when the point is inside the selection.
    FinishAt([f64; 2]),
    /// Annotation tool: start a new object inside the selection.
    Create,
    /// Select tool: a hit object starts its own move/resize in the core.
    ObjectDrag,
    /// Select tool: the core owns a selection move/resize.
    Drag,
    /// Press outside: the overlay shows a new-selection band committed on release.
    Band,
}

/// Applies I-09 priority for keys plus the I-08 undo/redo and delete shortcuts.
///
/// While the mouse button is held, keys are ignored so one drag stays one
/// undoable step. Output processing disables editing; Esc still cancels then,
/// because the document may not be edited while a job owns it.
#[must_use]
pub fn capture_key_action(
    key: &str,
    modifiers: KeyModifiers,
    context: CaptureKeyContext,
) -> Option<CaptureKeyAction> {
    let command_only = modifiers.command && !modifiers.control && !modifiers.option;
    let plain = !command_only && !modifiers.shift && !modifiers.control && !modifiers.option;
    match key {
        "escape" if context.dragging => None,
        "escape" if context.busy => Some(CaptureKeyAction::Cancel),
        "escape" if context.object_selected => Some(CaptureKeyAction::Deselect),
        "escape" if context.tool != Tool::Select => Some(CaptureKeyAction::SelectTool),
        "escape" => Some(CaptureKeyAction::Cancel),
        "enter" if !context.busy && !context.dragging => Some(CaptureKeyAction::Finish),
        "delete" | "backspace"
            if plain && context.object_selected && !context.busy && !context.dragging =>
        {
            Some(CaptureKeyAction::DeleteObject)
        }
        "z" if command_only && !modifiers.shift && !context.busy && !context.dragging => {
            Some(CaptureKeyAction::Undo)
        }
        "z" if command_only && modifiers.shift && !context.busy && !context.dragging => {
            Some(CaptureKeyAction::Redo)
        }
        _ => None,
    }
}

/// Routes one primary-button press; the core still validates the exact target.
///
/// With any annotation tool active (Rectangle, Line, Arrow, Blur) a press inside the
/// selection always starts a new object and a double-click never finishes. With
/// the select tool, an object body or control point wins over every selection
/// gesture and over Finish.
#[must_use]
pub fn capture_press_action(
    double_click: bool,
    busy: bool,
    tool: Tool,
    selection: Option<Selection>,
    object_hit: bool,
    point: [f64; 2],
) -> Option<CapturePressAction> {
    if busy {
        return None;
    }
    let inside = selection.is_some_and(|selection| selection.contains(point));
    if tool.is_annotation() {
        return Some(if inside {
            CapturePressAction::Create
        } else {
            CapturePressAction::Band
        });
    }
    let Some(selection) = selection else {
        return Some(CapturePressAction::Band);
    };
    if inside && object_hit {
        return Some(CapturePressAction::ObjectDrag);
    }
    if selection.resize_handle_at(point).is_some() {
        return Some(CapturePressAction::Drag);
    }
    if double_click {
        return Some(CapturePressAction::FinishAt(point));
    }
    if inside {
        Some(CapturePressAction::Drag)
    } else {
        Some(CapturePressAction::Band)
    }
}
