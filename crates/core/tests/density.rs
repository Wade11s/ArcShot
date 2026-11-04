//! CORE-DENSITY: density is authoritative session state, not a document edit.
//!
//! I-09: both densities must offer exactly the same options and switching must
//! never clear the active tool, per-tool styles, selection, objects or an
//! in-progress edit. Density lives on the session (outside the replaceable
//! `Document`), so the invariant is observable at the authoritative core seam.
use arcshot_core::{
    Annotation, CaptureAction, CaptureEffect, CaptureSession, CaptureTicket, CapturedFrame,
    Density, PaletteColor, RectangleOption, RectangleStyle, ScreenGeometry, StrokeWidth, Tool,
};

fn screen() -> ScreenGeometry {
    ScreenGeometry::try_new(1, 0.0, 0.0, 100.0, 50.0, 2.0).unwrap()
}

fn frame() -> CapturedFrame {
    let [w, h] = screen().pixel_size();
    CapturedFrame::try_from_srgb_bgra(
        screen(),
        w,
        h,
        w as usize * 4,
        vec![255; w as usize * h as usize * 4],
    )
    .unwrap()
}

fn frozen() -> (CaptureSession, CaptureTicket) {
    let mut session = CaptureSession::default();
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    session.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    (session, ticket)
}

/// Everything I-09 says a density switch must not disturb, read through public
/// document accessors only. In-progress text is not a product state in this
/// slice; an active pointer drag stands in for an in-progress edit.
fn document_snapshot(
    session: &CaptureSession,
) -> (u64, Vec<Annotation>, RectangleStyle, Tool, bool) {
    let document = session.document().expect("frozen session owns a document");
    (
        document.version(),
        document.annotations().to_vec(),
        document.rectangle_style(),
        document.tool(),
        document.dragging(),
    )
}

#[test]
fn core_density_01_default_compact_round_trips_and_survives_a_new_session() {
    assert_eq!(
        Density::default(),
        Density::Compact,
        "I-09 default is Compact"
    );

    // The stored token is a stable, pure codec; an unknown token is a failure.
    for density in [Density::Standard, Density::Compact] {
        assert_eq!(Density::from_stored(density.as_str()), Some(density));
    }
    assert_eq!(Density::from_stored("Bogus"), None);
    assert_eq!(Density::Standard.toggled(), Density::Compact);
    assert_eq!(Density::Compact.toggled(), Density::Standard);

    let mut session = CaptureSession::default();
    assert_eq!(session.density(), Density::Compact);
    session.set_density(Density::Standard);
    assert_eq!(session.density(), Density::Standard);
    session.set_density(Density::Compact);
    assert_eq!(session.density(), Density::Compact);

    // A new capture replaces the Document but keeps the session preference,
    // exactly like the completion-sound choice (I-04 "跨会话/重启记忆").
    session.set_density(Density::Standard);
    let (mut session, ticket) = frozen_from(session);
    assert_eq!(session.density(), Density::Standard);
    assert!(session.document().is_some());
    session.dispatch(CaptureAction::Cancel);
    assert_eq!(
        session.density(),
        Density::Standard,
        "a cancelled session keeps the preference"
    );
    let _ = ticket;
}

/// Runs one more capture on an existing session (which already holds a density).
fn frozen_from(mut session: CaptureSession) -> (CaptureSession, CaptureTicket) {
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("expected permission check");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    session.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    (session, ticket)
}

#[test]
fn core_density_02_switching_density_preserves_tool_style_selection_objects_and_edit() {
    let (mut session, ticket) = frozen();
    // A valid selection, a non-default per-tool style and two objects.
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    session.dispatch(CaptureAction::SelectTool {
        ticket,
        tool: Tool::Rectangle,
    });
    session.dispatch(CaptureAction::RectangleOption {
        ticket,
        option: RectangleOption::Width(StrokeWidth::Five),
    });
    session.dispatch(CaptureAction::RectangleOption {
        ticket,
        option: RectangleOption::Stroke(PaletteColor::Blue),
    });
    session.dispatch(CaptureAction::RectangleOption {
        ticket,
        option: RectangleOption::Fill(true),
    });
    for (from, to) in [([12.0, 8.0], [22.0, 18.0]), ([24.0, 10.0], [34.0, 20.0])] {
        session.dispatch(CaptureAction::PointerPressed {
            ticket,
            point: from,
        });
        session.dispatch(CaptureAction::PointerMoved { ticket, point: to });
        session.dispatch(CaptureAction::PointerReleased { ticket });
    }
    // An in-progress drag: standing in for in-progress text/IME composition.
    session.dispatch(CaptureAction::PointerPressed {
        ticket,
        point: [15.0, 9.0],
    });
    assert!(session.document().unwrap().dragging());

    let before = document_snapshot(&session);
    assert_eq!(before.1.len(), 2, "two objects were created");
    assert_eq!(before.3, Tool::Rectangle);
    assert!(before.4, "an edit is in progress");

    // Switching density in either direction, repeatedly, changes no document
    // state and emits no capture effects: it is a pure view preference.
    for density in [
        Density::Standard,
        Density::Compact,
        Density::Standard,
        Density::Compact,
    ] {
        session.set_density(density);
        assert_eq!(session.density(), density);
        assert_eq!(document_snapshot(&session), before, "density {density:?}");
    }
    assert_eq!(
        session.document().unwrap().selection(),
        Some(arcshot_core::Selection::try_new(10.0, 5.0, 30.0, 20.0).unwrap())
    );
    assert_eq!(
        session.document().unwrap().rectangle_style(),
        RectangleStyle::new(PaletteColor::Blue, StrokeWidth::Five, true)
    );
}
