//! APP-EFFECT: routing Scroll tap commands into authoritative session actions.
//!
//! The session tap reports raw user intents (finish / cancel). This policy is
//! the only translator: a finish needs an open collection and an idle output
//! slot; a cancel works in every active Scroll phase; nothing routes with a
//! stale ticket.
use arcshot_app::{ScrollTapCommand, scroll_tap_action};
use arcshot_core::{CaptureAction, CaptureEffect, CapturePhase, CaptureSession, ScrollPhase};

/// One live ticket from a real session start; tickets are never invented.
fn ticket() -> arcshot_core::CaptureTicket {
    let mut session = CaptureSession::default();
    let effects = session.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = &effects[0] else {
        panic!("expected a permission check");
    };
    *ticket
}

fn active_phases() -> [ScrollPhase; 5] {
    [
        ScrollPhase::CheckingPermissions,
        ScrollPhase::IdentifyingTarget,
        ScrollPhase::PreparingWindows,
        ScrollPhase::Prepared,
        ScrollPhase::Collecting,
    ]
}

#[test]
fn app_effect_42_finish_routes_only_from_collecting_and_an_idle_output() {
    let ticket = ticket();
    // Only an open collection may complete; every earlier phase ignores Enter.
    for phase in active_phases() {
        let action = scroll_tap_action(
            phase,
            CapturePhase::Frozen,
            false,
            ticket,
            ScrollTapCommand::Finish,
        );
        if phase == ScrollPhase::Collecting {
            assert_eq!(action, Some(CaptureAction::CompleteScroll { ticket }));
        } else {
            assert_eq!(action, None);
        }
    }
    // While a completion output runs, a second Enter is dropped, not queued.
    assert_eq!(
        scroll_tap_action(
            ScrollPhase::Collecting,
            CapturePhase::Frozen,
            true,
            ticket,
            ScrollTapCommand::Finish
        ),
        None
    );
    assert_eq!(
        scroll_tap_action(
            ScrollPhase::Inactive,
            CapturePhase::Resident,
            false,
            ticket,
            ScrollTapCommand::Finish
        ),
        None
    );
}

#[test]
fn app_effect_43_cancel_routes_from_every_active_scroll_phase() {
    let ticket = ticket();
    for phase in active_phases() {
        assert_eq!(
            scroll_tap_action(
                phase,
                CapturePhase::Frozen,
                false,
                ticket,
                ScrollTapCommand::Cancel
            ),
            Some(CaptureAction::CancelScroll { ticket })
        );
    }
    // Esc during an in-flight completion output still cancels (abandon).
    assert_eq!(
        scroll_tap_action(
            ScrollPhase::Collecting,
            CapturePhase::Frozen,
            true,
            ticket,
            ScrollTapCommand::Cancel
        ),
        Some(CaptureAction::CancelScroll { ticket })
    );
    // With no Scroll preparation, Esc belongs to the region session instead.
    assert_eq!(
        scroll_tap_action(
            ScrollPhase::Inactive,
            CapturePhase::Frozen,
            false,
            ticket,
            ScrollTapCommand::Cancel
        ),
        None
    );
}

#[test]
fn app_effect_44_actions_carry_the_live_ticket_and_never_invent_a_phase() {
    let live = ticket();
    for (phase, command, routed) in [
        (ScrollPhase::Collecting, ScrollTapCommand::Finish, true),
        (ScrollPhase::Collecting, ScrollTapCommand::Cancel, true),
        (ScrollPhase::Prepared, ScrollTapCommand::Finish, false),
        (ScrollPhase::Stopped, ScrollTapCommand::Cancel, false),
        (ScrollPhase::Inactive, ScrollTapCommand::Cancel, false),
    ] {
        let action = scroll_tap_action(phase, CapturePhase::Frozen, false, live, command);
        assert_eq!(routed, action.is_some());
        if let Some(action) = action {
            let stamped = match action {
                CaptureAction::CompleteScroll { ticket }
                | CaptureAction::CancelScroll { ticket } => ticket,
                _ => panic!("unexpected routed action: {action:?}"),
            };
            assert_eq!(stamped, live);
        }
    }
    // The coordinator stamps the live preparation ticket before calling; the
    // stale-command fence itself is CORE-SESSION-59.
}
