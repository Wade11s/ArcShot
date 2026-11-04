//! Routing Scroll session-tap commands into authoritative session actions.
//!
//! The Quartz tap reports raw user intents observed behind the input
//! protection; this pure policy is the only translator into core actions. A
//! finish needs an open collection and an idle output slot (no repeated
//! completion while output runs); a cancel reaches the core in every active
//! Scroll phase, including while a completion output is in flight, where the
//! core abandons the publish fence. The coordinator stamps the live
//! preparation ticket; stale commands never reach this function (and the core
//! fences them again by ticket and phase).
use arcshot_core::{CaptureAction, CapturePhase, CaptureTicket, ScrollPhase};

/// A user intent observed by the session event tap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollTapCommand {
    /// Enter or an ArcShot-owned double-click asked to complete.
    Finish,
    /// Escape asked to abandon the scroll result.
    Cancel,
}

/// Maps one tap command to the authoritative action for the live preparation.
///
/// `output_busy` mirrors the session's shared output mutex; `phase` is the
/// session's live Scroll phase snapshot.
#[must_use]
pub fn scroll_tap_action(
    phase: ScrollPhase,
    capture_phase: CapturePhase,
    output_busy: bool,
    ticket: CaptureTicket,
    command: ScrollTapCommand,
) -> Option<CaptureAction> {
    // The scroll loop only exists inside a frozen region session and an
    // active preparation; anything else means the command arrived after the
    // session moved on.
    let active = matches!(
        phase,
        ScrollPhase::CheckingPermissions
            | ScrollPhase::IdentifyingTarget
            | ScrollPhase::PreparingWindows
            | ScrollPhase::Prepared
            | ScrollPhase::Collecting
    );
    if !active || capture_phase != CapturePhase::Frozen {
        return None;
    }
    match command {
        ScrollTapCommand::Finish => {
            // Repeated completion is disabled while an output runs (I-11 §9).
            (phase == ScrollPhase::Collecting && !output_busy)
                .then_some(CaptureAction::CompleteScroll { ticket })
        }
        // Esc abandons the scroll result from every active phase, including
        // while a completion output is in flight; the core cancels the fence.
        ScrollTapCommand::Cancel => Some(CaptureAction::CancelScroll { ticket }),
    }
}
