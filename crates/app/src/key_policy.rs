//! Overlay key priority before semantic dispatch; drag state is view-only.

/// What the overlay should queue for a key press, if anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureKeyAction {
    /// Esc: cancel the session without output.
    Cancel,
    /// Enter: finish the current valid selection.
    Finish,
}

/// Applies I-09 priority for the keys implemented in this slice.
///
/// While the mouse button is held, both keys are ignored so a drag stays
/// undoable instead of cancelling or finishing a previous selection. Enter is
/// also inert while output/save processing owns the mutex; Esc remains usable
/// and its late result is fenced by the document version.
#[must_use]
pub fn capture_key_action(key: &str, busy: bool, dragging: bool) -> Option<CaptureKeyAction> {
    match key {
        "escape" if !dragging => Some(CaptureKeyAction::Cancel),
        "enter" if !busy && !dragging => Some(CaptureKeyAction::Finish),
        _ => None,
    }
}
