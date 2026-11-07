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
    /// A text editor is active; Enter, Delete/Backspace and undo/redo belong to it.
    pub text_editing: bool,
    /// The system input method owns a marked (preedit) composition.
    ///
    /// While composing, Esc cancels the candidate, Enter selects one, and
    /// digits/space pick candidates. None of those may be turned into an
    /// overlay action; the IME must see them first.
    pub ime_composing: bool,
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
    /// Esc layer zero while text is active: end editing and keep the content.
    EndTextEdit,
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
    /// Double-click on a text object under the select tool: re-edit it.
    EditText([f64; 2]),
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
    // While a text editor is active, Enter, Delete/Backspace and undo/redo stay
    // with the editor and IME. Esc ends the edit first; every other application
    // shortcut is suppressed so IME composition keys keep priority.
    if context.text_editing && !context.busy && !context.dragging {
        if context.ime_composing {
            // The IME owns every key until the composition ends; Esc must
            // cancel the candidate, not commit the raw Pinyin and close the
            // editor.
            return None;
        }
        return match key {
            "escape" => Some(CaptureKeyAction::EndTextEdit),
            _ => None,
        };
    }
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

/// Routes one press on the whole-image completion annotation surface.
///
/// Unlike [`capture_press_action`] this never moves, resizes or reselects the
/// full-frame selection: the long image is output at its document extent, so a
/// press may only create an annotation, drag a hit object, or do nothing.
#[must_use]
pub fn capture_long_image_press_action(
    _double_click: bool,
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
        return inside.then_some(CapturePressAction::Create);
    }
    (inside && object_hit).then_some(CapturePressAction::ObjectDrag)
}

/// Decides whether a press is a text re-edit under the select tool.
///
/// A double-click on a text object re-edits it; every other object keeps the
/// normal object drag. The text tool always creates, even on a double-click.
#[must_use]
pub fn capture_text_press_action(
    double_click: bool,
    tool: Tool,
    object_is_text: bool,
    point: [f64; 2],
) -> Option<CapturePressAction> {
    if double_click && tool == Tool::Select && object_is_text {
        return Some(CapturePressAction::EditText(point));
    }
    None
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
