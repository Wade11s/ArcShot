//! Explicit partial-result choice policy after a known-state Scroll stop.
//!
//! The authoritative session decides when collection stops and whether a
//! verified result exists. This pure policy only classifies that outcome
//! for presentation: a known-state refusal with verified history offers
//! exactly two choices (use the verified partial, or abandon); without
//! verified history, or when native state cannot be confirmed, only
//! abandon is offered. Nothing here outputs anything by itself.
use arcshot_core::{
    CaptureAction, CaptureTicket, ScrollRefusal, ScrollTargetError, ScrollWindowError,
};

/// Label of the explicit accept choice.
pub const PARTIAL_ACCEPT_LABEL: &str = "使用已验证部分";
/// Label of the abandon choice.
pub const PARTIAL_ABANDON_LABEL: &str = "放弃并返回原选区";
/// Warning shown with the accept choice: the result is incomplete and
/// copying waits for explicit confirmation.
pub const PARTIAL_INCOMPLETE_WARNING: &str = "已验证部分不完整；确认后才会复制。";

/// One explicit choice the user may make after a known-state stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollPartialChoice {
    /// Use the verified partial result (incomplete; copies only on confirm).
    UseVerifiedPartial,
    /// Abandon the partial and return the pre-scroll selection.
    Abandon,
}

/// Whether a refusal is a known state that may still offer a partial.
///
/// Unconfirmable native state (target/window identity) is never known:
/// it forbids partial output exactly like the terminal uncertain paths.
#[must_use]
pub fn is_known_state_refusal(refusal: &ScrollRefusal) -> bool {
    !matches!(
        refusal,
        ScrollRefusal::Target(ScrollTargetError::NativeUncertain)
            | ScrollRefusal::Windows(ScrollWindowError::NativeUncertain)
            | ScrollRefusal::Windows(ScrollWindowError::NotMainThread)
    )
}

/// Whether the verified partial may be offered for this refusal.
#[must_use]
pub fn partial_offered(has_verified: bool, refusal: &ScrollRefusal) -> bool {
    has_verified && is_known_state_refusal(refusal)
}

/// The exact choices offered: two with a verified partial, else abandon only.
#[must_use]
pub fn partial_choices(has_verified: bool, refusal: &ScrollRefusal) -> Vec<ScrollPartialChoice> {
    if partial_offered(has_verified, refusal) {
        vec![
            ScrollPartialChoice::UseVerifiedPartial,
            ScrollPartialChoice::Abandon,
        ]
    } else {
        vec![ScrollPartialChoice::Abandon]
    }
}

/// Maps one explicit choice to its authoritative session action.
#[must_use]
pub fn partial_action(choice: ScrollPartialChoice, ticket: CaptureTicket) -> CaptureAction {
    match choice {
        ScrollPartialChoice::UseVerifiedPartial => CaptureAction::AcceptScrollPartial { ticket },
        ScrollPartialChoice::Abandon => CaptureAction::AbandonScrollPartial { ticket },
    }
}
