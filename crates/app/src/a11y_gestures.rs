//! The overlay's accessibility gesture vocabulary.
//!
//! Selection and annotation gestures cannot be replayed as drags by assistive
//! technology, so each addressable affordance exposes its position as an AX
//! value and accepts a replacement through `SetValue`. The replacement is
//! translated here into the exact `UiCommand` sequence a pointer press, move
//! and release at the same points would enqueue — the same authoritative
//! capture path, never a second editor (#63).

use arcshot_app::{CapturePressAction, capture_long_image_press_action, capture_press_action};
use arcshot_core::{Annotation, CaptureTicket, Endpoint, ResizeHandle, Selection, Tool};

use crate::ui::UiCommand;

/// One parsed `SetValue` payload: a point affordance or two corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum A11yGeometry {
    /// A single document point: "x,y".
    Point([f64; 2]),
    /// Two document points: "x1,y1,x2,y2" (rectangle corners or a segment).
    Corners([f64; 2], [f64; 2]),
}

/// The overlay state one gesture is dispatched against.
pub(crate) struct GestureContext<'a> {
    /// Output processing owns the document; input must not reach it.
    pub(crate) busy: bool,
    /// Whether the whole-image completion surface is showing.
    pub(crate) long_image: bool,
    /// The active pointer tool.
    pub(crate) tool: Tool,
    /// The authoritative selection mirrored from the document.
    pub(crate) selection: Option<Selection>,
    /// The layer-ordered objects mirrored from the document.
    pub(crate) annotations: &'a [Annotation],
    /// Whether an in-place text edit is open; a press commits it first.
    pub(crate) text_editing: bool,
}

/// Where one affordance's press lands, resolved against the live mirrors.
#[derive(Clone, Copy, Debug)]
pub(crate) enum A11yAnchor {
    /// The center of the current selection.
    SelectionCenter,
    /// One of the selection's eight resize control points.
    SelectionHandle(ResizeHandle),
    /// The center of one annotation object's retained geometry.
    ObjectCenter(usize),
    /// One rectangle control point of the selected object.
    ObjectHandle { index: usize, handle: ResizeHandle },
    /// One endpoint of a selected line or arrow.
    Endpoint { index: usize, endpoint: Endpoint },
}

/// How one node's AX payload maps onto pointer-equivalent commands.
#[derive(Clone, Copy, Debug)]
pub(crate) enum A11yAffordance {
    /// `SetValue` carries a point: press at the anchor, drag to the payload.
    DragFrom(A11yAnchor),
    /// `SetValue` carries corners: the band-release reselect command.
    Band,
    /// `SetValue` carries corners: press at the first, drag to the second.
    CornersGesture,
    /// `Click`: a zero-length press at the anchor (selects an object).
    Click(A11yAnchor),
    /// `SetValue` carries corners: move the object's center to their middle.
    MoveObject(usize),
}

/// The stable `accessibility_id` suffix of one selection or object control point.
///
/// These names are part of the E2E contract (#63): changing one breaks the
/// recorded cases, so they are spelled out rather than derived from `Debug`.
#[must_use]
pub(crate) const fn handle_name(handle: ResizeHandle) -> &'static str {
    match handle {
        ResizeHandle::TopLeft => "top_left",
        ResizeHandle::Top => "top",
        ResizeHandle::TopRight => "top_right",
        ResizeHandle::Right => "right",
        ResizeHandle::BottomRight => "bottom_right",
        ResizeHandle::Bottom => "bottom",
        ResizeHandle::BottomLeft => "bottom_left",
        ResizeHandle::Left => "left",
    }
}

/// The stable `accessibility_id` suffix of one line or arrow endpoint.
#[must_use]
pub(crate) const fn endpoint_name(endpoint: Endpoint) -> &'static str {
    match endpoint {
        Endpoint::Start => "start",
        Endpoint::End => "end",
    }
}

/// The retained geometry of one annotation as its two AX corners.
///
/// Rectangles and blur give their bounds' corners, lines and arrows their
/// endpoints, text the corners of its measured box. Nothing is clipped to the
/// selection; the document stays the authority for output cropping.
#[must_use]
pub(crate) fn annotation_corners(annotation: &Annotation) -> Option<([f64; 2], [f64; 2])> {
    if let Some(bounds) = annotation.shape().rectangle_bounds() {
        return Some((
            [bounds.x(), bounds.y()],
            [bounds.x() + bounds.width(), bounds.y() + bounds.height()],
        ));
    }
    if let Some(text) = annotation.shape().text() {
        let bounds = text.bounds();
        return Some((
            [bounds.x(), bounds.y()],
            [bounds.x() + bounds.width(), bounds.y() + bounds.height()],
        ));
    }
    let segment = annotation.shape().segment()?;
    Some((
        segment.endpoint_center(Endpoint::Start),
        segment.endpoint_center(Endpoint::End),
    ))
}

/// Resolves one affordance's press point against the live mirrors.
#[must_use]
pub(crate) fn anchor_point(context: &GestureContext<'_>, anchor: A11yAnchor) -> Option<[f64; 2]> {
    match anchor {
        A11yAnchor::SelectionCenter => context.selection.map(|selection| {
            [
                selection.x() + selection.width() / 2.0,
                selection.y() + selection.height() / 2.0,
            ]
        }),
        A11yAnchor::SelectionHandle(handle) => context
            .selection
            .map(|selection| selection.handle_center(handle)),
        A11yAnchor::ObjectCenter(index) => {
            let (a, b) = annotation_corners(context.annotations.get(index)?)?;
            Some([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
        }
        A11yAnchor::ObjectHandle { index, handle } => {
            context.annotations.get(index)?.handle_center(handle)
        }
        A11yAnchor::Endpoint { index, endpoint } => context
            .annotations
            .get(index)?
            .shape()
            .segment()
            .map(|segment| segment.endpoint_center(endpoint)),
    }
}

/// Parses a comma-separated AX value payload into document points.
#[must_use]
pub(crate) fn parse_geometry(text: &str) -> Option<A11yGeometry> {
    let mut fields = text.split(',');
    let mut number = || -> Option<f64> {
        let value: f64 = fields.next()?.trim().parse().ok()?;
        value.is_finite().then_some(value)
    };
    let first = [number()?, number()?];
    match number() {
        None => Some(A11yGeometry::Point(first)),
        Some(x2) => {
            let second = [x2, number()?];
            // A trailing field means the payload is neither point nor corners.
            fields
                .next()
                .is_none()
                .then_some(A11yGeometry::Corners(first, second))
        }
    }
}

/// Formats one document point as the canonical AX value form.
#[must_use]
pub(crate) fn format_point(point: [f64; 2]) -> String {
    format!("{},{}", point[0], point[1])
}

/// Formats two document points as the canonical AX value form.
#[must_use]
pub(crate) fn format_corners(a: [f64; 2], b: [f64; 2]) -> String {
    format!("{},{},{},{}", a[0], a[1], b[0], b[1])
}

/// Builds the pointer-equivalent commands for one press-drag-release gesture.
///
/// The translation reuses the same pure press dispatchers the overlay's mouse
/// handler runs (`capture_press_action` / `capture_long_image_press_action`),
/// so an AX gesture and a pointer gesture at the same points enqueue the same
/// commands. Double-click-only actions never arise: an AX gesture is a single
/// press.
#[must_use]
pub(crate) fn press_drag_release(
    context: &GestureContext<'_>,
    ticket: CaptureTicket,
    from: [f64; 2],
    to: [f64; 2],
) -> Vec<UiCommand> {
    // A press while editing commits the draft; the same press starts nothing.
    if context.text_editing {
        return vec![UiCommand::TextCommit(ticket)];
    }
    let object_hit = context
        .selection
        .filter(|selection| selection.contains(from))
        .and_then(|_| arcshot_core::annotation_at(context.annotations, from))
        .is_some();
    let action = if context.long_image {
        capture_long_image_press_action(
            false,
            context.busy,
            context.tool,
            context.selection,
            object_hit,
            from,
        )
    } else {
        capture_press_action(
            false,
            context.busy,
            context.tool,
            context.selection,
            object_hit,
            from,
        )
    };
    match action {
        // The text tool positions an editor instead of a drag.
        Some(CapturePressAction::Create) if context.tool == Tool::Text => {
            vec![UiCommand::BeginText {
                ticket,
                point: from,
            }]
        }
        Some(
            CapturePressAction::Create | CapturePressAction::ObjectDrag | CapturePressAction::Drag,
        ) => vec![
            UiCommand::PointerPressed {
                ticket,
                point: from,
            },
            UiCommand::PointerMoved { ticket, point: to },
            UiCommand::PointerReleased(ticket),
        ],
        Some(CapturePressAction::Band) => vec![UiCommand::SelectRegion { ticket, from, to }],
        // Double-click-only resolutions never arise from a single AX press.
        Some(CapturePressAction::FinishAt(_) | CapturePressAction::EditText(_)) | None => {
            Vec::new()
        }
    }
}

/// Builds the pointer-equivalent commands for one AX activation.
///
/// `payload` is the raw AX value text: empty for `Click`, otherwise the
/// geometry the affordance expects. A payload of the wrong shape produces no
/// commands, so a malformed value can never half-apply a gesture.
#[must_use]
pub(crate) fn affordance_commands(
    context: &GestureContext<'_>,
    ticket: CaptureTicket,
    affordance: A11yAffordance,
    payload: &str,
) -> Vec<UiCommand> {
    if context.busy {
        return Vec::new();
    }
    match affordance {
        A11yAffordance::Band => match parse_geometry(payload) {
            Some(A11yGeometry::Corners(from, to)) => {
                vec![UiCommand::SelectRegion { ticket, from, to }]
            }
            Some(A11yGeometry::Point(_)) | None => Vec::new(),
        },
        A11yAffordance::CornersGesture => match parse_geometry(payload) {
            Some(A11yGeometry::Corners(from, to)) => press_drag_release(context, ticket, from, to),
            Some(A11yGeometry::Point(_)) | None => Vec::new(),
        },
        A11yAffordance::DragFrom(anchor) => match parse_geometry(payload) {
            Some(A11yGeometry::Point(to)) => anchor_point(context, anchor)
                .map(|from| press_drag_release(context, ticket, from, to))
                .unwrap_or_default(),
            Some(A11yGeometry::Corners(_, _)) | None => Vec::new(),
        },
        A11yAffordance::Click(anchor) => anchor_point(context, anchor)
            .map(|point| press_drag_release(context, ticket, point, point))
            .unwrap_or_default(),
        A11yAffordance::MoveObject(index) => {
            let Some(A11yGeometry::Corners(a, b)) = parse_geometry(payload) else {
                return Vec::new();
            };
            let Some(from) = anchor_point(context, A11yAnchor::ObjectCenter(index)) else {
                return Vec::new();
            };
            let to = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            press_drag_release(context, ticket, from, to)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        A11yAffordance, A11yAnchor, A11yGeometry, GestureContext, affordance_commands,
        anchor_point, annotation_corners, endpoint_name, format_corners, format_point, handle_name,
        parse_geometry, press_drag_release,
    };
    use crate::ui::UiCommand;
    use arcshot_core::{
        Annotation, CaptureAction, CaptureEffect, CaptureSession, CaptureTicket, LineStyle,
        RectangleStyle, Segment, Selection, Tool,
    };

    fn ticket() -> CaptureTicket {
        let mut session = CaptureSession::default();
        session
            .dispatch(CaptureAction::Start)
            .into_iter()
            .find_map(|effect| match effect {
                CaptureEffect::CheckPermission(ticket) => Some(ticket),
                _ => None,
            })
            .expect("starting a session issues a ticket")
    }

    fn context(
        tool: Tool,
        selection: Option<Selection>,
        annotations: &[Annotation],
    ) -> GestureContext<'_> {
        GestureContext {
            busy: false,
            long_image: false,
            tool,
            selection,
            annotations,
            text_editing: false,
        }
    }

    fn selection() -> Selection {
        Selection::try_new(100.0, 100.0, 400.0, 300.0).expect("valid selection")
    }

    fn rectangle_at(x: f64, y: f64, width: f64, height: f64) -> Annotation {
        let bounds = Selection::try_new(x, y, width, height).expect("valid bounds");
        Annotation::rectangle(bounds, RectangleStyle::default())
    }

    fn line_from(start: [f64; 2], end: [f64; 2]) -> Annotation {
        let segment = Segment::from_drag(start, end).expect("valid segment");
        Annotation::line(segment, LineStyle::default())
    }

    #[test]
    fn anchors_resolve_against_the_live_mirrors() {
        let ticket = ticket();
        let _ = ticket;
        let annotations = [
            rectangle_at(150.0, 150.0, 80.0, 60.0),
            line_from([10.0, 20.0], [210.0, 260.0]),
        ];
        let live = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            anchor_point(&live, A11yAnchor::SelectionCenter),
            Some([300.0, 250.0])
        );
        assert_eq!(
            anchor_point(
                &live,
                A11yAnchor::SelectionHandle(arcshot_core::ResizeHandle::BottomRight)
            ),
            Some([500.0, 400.0])
        );
        assert_eq!(
            anchor_point(&live, A11yAnchor::ObjectCenter(0)),
            Some([190.0, 180.0])
        );
        assert_eq!(
            anchor_point(
                &live,
                A11yAnchor::ObjectHandle {
                    index: 0,
                    handle: arcshot_core::ResizeHandle::TopLeft
                }
            ),
            Some([150.0, 150.0])
        );
        assert_eq!(
            anchor_point(
                &live,
                A11yAnchor::Endpoint {
                    index: 1,
                    endpoint: arcshot_core::Endpoint::End
                }
            ),
            Some([210.0, 260.0])
        );
        // Out-of-range objects and missing selections resolve to nothing.
        assert_eq!(anchor_point(&live, A11yAnchor::ObjectCenter(9)), None);
        let empty: [Annotation; 0] = [];
        let no_selection = context(Tool::Select, None, &empty);
        assert_eq!(
            anchor_point(
                &no_selection,
                A11yAnchor::SelectionHandle(arcshot_core::ResizeHandle::Top)
            ),
            None
        );
    }

    #[test]
    fn object_corners_come_from_the_retained_geometry() {
        let rectangle = rectangle_at(150.0, 150.0, 80.0, 60.0);
        assert_eq!(
            annotation_corners(&rectangle),
            Some(([150.0, 150.0], [230.0, 210.0]))
        );
        let line = line_from([10.0, 20.0], [210.0, 260.0]);
        assert_eq!(
            annotation_corners(&line),
            Some(([10.0, 20.0], [210.0, 260.0]))
        );
    }

    #[test]
    fn parse_accepts_two_and_four_finite_numbers() {
        assert_eq!(
            parse_geometry("200,300"),
            Some(A11yGeometry::Point([200.0, 300.0]))
        );
        assert_eq!(
            parse_geometry(" 200 , 300 , 500 , 400 "),
            Some(A11yGeometry::Corners([200.0, 300.0], [500.0, 400.0]))
        );
        assert_eq!(
            parse_geometry("-20.5,0"),
            Some(A11yGeometry::Point([-20.5, 0.0]))
        );
    }

    #[test]
    fn parse_rejects_other_shapes_and_non_finite_values() {
        assert_eq!(parse_geometry(""), None);
        assert_eq!(parse_geometry("200"), None);
        assert_eq!(parse_geometry("200,300,500"), None);
        assert_eq!(parse_geometry("200,300,500,400,600"), None);
        assert_eq!(parse_geometry("200,abc"), None);
        assert_eq!(parse_geometry("nan,300"), None);
        assert_eq!(parse_geometry("inf,300"), None);
        assert_eq!(parse_geometry("200,,300"), None);
    }

    #[test]
    fn formats_use_shortest_float_display() {
        assert_eq!(format_point([200.0, 300.0]), "200,300");
        assert_eq!(format_point([12.5, -0.25]), "12.5,-0.25");
        assert_eq!(
            format_corners([100.0, 120.0], [300.0, 260.0]),
            "100,120,300,260"
        );
    }

    #[test]
    fn select_tool_inside_the_selection_moves_it_through_the_pointer_sequence() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let context = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&context, ticket, [200.0, 150.0], [260.0, 210.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [200.0, 150.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [260.0, 210.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn select_tool_outside_the_selection_reselects_like_a_band_release() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let context = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&context, ticket, [20.0, 30.0], [80.0, 90.0]),
            vec![UiCommand::SelectRegion {
                ticket,
                from: [20.0, 30.0],
                to: [80.0, 90.0],
            }]
        );
    }

    #[test]
    fn without_a_selection_every_surface_press_is_a_band() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let context = context(Tool::Select, None, &annotations);
        assert_eq!(
            press_drag_release(&context, ticket, [10.0, 10.0], [50.0, 50.0]),
            vec![UiCommand::SelectRegion {
                ticket,
                from: [10.0, 10.0],
                to: [50.0, 50.0],
            }]
        );
    }

    #[test]
    fn an_object_body_press_starts_an_object_drag() {
        let ticket = ticket();
        let annotations = [rectangle_at(150.0, 150.0, 80.0, 60.0)];
        let context = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&context, ticket, [190.0, 180.0], [210.0, 220.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [190.0, 180.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [210.0, 220.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn annotation_tools_create_inside_and_band_outside() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let inside = context(Tool::Rectangle, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&inside, ticket, [200.0, 150.0], [300.0, 200.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [200.0, 150.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [300.0, 200.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
        let outside = context(Tool::Rectangle, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&outside, ticket, [20.0, 20.0], [90.0, 90.0]),
            vec![UiCommand::SelectRegion {
                ticket,
                from: [20.0, 20.0],
                to: [90.0, 90.0],
            }]
        );
    }

    #[test]
    fn the_text_tool_creates_with_the_pointer_click_command() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let context = context(Tool::Text, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&context, ticket, [200.0, 150.0], [400.0, 300.0]),
            vec![UiCommand::BeginText {
                ticket,
                point: [200.0, 150.0],
            }]
        );
    }

    #[test]
    fn busy_input_produces_no_commands() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let mut context = context(Tool::Select, Some(selection()), &annotations);
        context.busy = true;
        assert_eq!(
            press_drag_release(&context, ticket, [200.0, 150.0], [260.0, 210.0]),
            Vec::new()
        );
    }

    #[test]
    fn a_press_while_text_editing_only_commits_the_draft() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let mut context = context(Tool::Select, Some(selection()), &annotations);
        context.text_editing = true;
        assert_eq!(
            press_drag_release(&context, ticket, [200.0, 150.0], [260.0, 210.0]),
            vec![UiCommand::TextCommit(ticket)]
        );
    }

    #[test]
    fn the_long_image_surface_never_bands_and_only_edits_objects() {
        let ticket = ticket();
        let annotations = [rectangle_at(150.0, 150.0, 80.0, 60.0)];
        let mut context = context(Tool::Select, Some(selection()), &annotations);
        context.long_image = true;
        // An object body still drags.
        assert_eq!(
            press_drag_release(&context, ticket, [190.0, 180.0], [210.0, 220.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [190.0, 180.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [210.0, 220.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
        // Empty space does nothing; there is no band on the whole image.
        assert_eq!(
            press_drag_release(&context, ticket, [120.0, 120.0], [200.0, 200.0]),
            Vec::new()
        );
        // Annotation tools still create inside the selection.
        context.tool = Tool::Rectangle;
        assert_eq!(
            press_drag_release(&context, ticket, [200.0, 150.0], [300.0, 200.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [200.0, 150.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [300.0, 200.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn line_and_blur_annotations_are_created_through_the_same_sequence() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let line = context(Tool::Line, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&line, ticket, [150.0, 120.0], [420.0, 260.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [150.0, 120.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [420.0, 260.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
        let blur = context(Tool::Blur, Some(selection()), &annotations);
        assert_eq!(
            press_drag_release(&blur, ticket, [150.0, 120.0], [420.0, 260.0]),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [150.0, 120.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [420.0, 260.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn the_selection_node_reselects_from_a_corner_payload() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let live = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            affordance_commands(&live, ticket, A11yAffordance::Band, "20,30,80,90"),
            vec![UiCommand::SelectRegion {
                ticket,
                from: [20.0, 30.0],
                to: [80.0, 90.0],
            }]
        );
        // A point payload is not a band; nothing is applied.
        assert_eq!(
            affordance_commands(&live, ticket, A11yAffordance::Band, "20,30"),
            Vec::new()
        );
        assert_eq!(
            affordance_commands(&live, ticket, A11yAffordance::Band, "garbage"),
            Vec::new()
        );
    }

    #[test]
    fn a_selection_handle_drag_starts_at_the_handle_not_the_payload() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let live = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            affordance_commands(
                &live,
                ticket,
                A11yAffordance::DragFrom(A11yAnchor::SelectionHandle(
                    arcshot_core::ResizeHandle::BottomRight
                )),
                "620,520"
            ),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [500.0, 400.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [620.0, 520.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn a_click_presses_and_releases_at_the_anchor() {
        let ticket = ticket();
        let annotations = [rectangle_at(150.0, 150.0, 80.0, 60.0)];
        let live = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            affordance_commands(
                &live,
                ticket,
                A11yAffordance::Click(A11yAnchor::ObjectCenter(0)),
                ""
            ),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [190.0, 180.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [190.0, 180.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
        // A click on a missing object does nothing.
        assert_eq!(
            affordance_commands(
                &live,
                ticket,
                A11yAffordance::Click(A11yAnchor::ObjectCenter(4)),
                ""
            ),
            Vec::new()
        );
    }

    #[test]
    fn moving_an_object_drags_its_center_to_the_payload_middle() {
        let ticket = ticket();
        let annotations = [rectangle_at(150.0, 150.0, 80.0, 60.0)];
        let live = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            affordance_commands(
                &live,
                ticket,
                A11yAffordance::MoveObject(0),
                "250,250,330,310"
            ),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [190.0, 180.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [290.0, 280.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn an_endpoint_affordance_drags_from_the_endpoint_to_the_payload() {
        let ticket = ticket();
        let annotations = [line_from([10.0, 20.0], [210.0, 260.0])];
        let live = context(Tool::Select, Some(selection()), &annotations);
        assert_eq!(
            affordance_commands(
                &live,
                ticket,
                A11yAffordance::DragFrom(A11yAnchor::Endpoint {
                    index: 0,
                    endpoint: arcshot_core::Endpoint::End,
                }),
                "300,320"
            ),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [210.0, 260.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [300.0, 320.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn a_busy_surface_ignores_every_affordance() {
        let ticket = ticket();
        let annotations = [rectangle_at(150.0, 150.0, 80.0, 60.0)];
        let mut live = context(Tool::Select, Some(selection()), &annotations);
        live.busy = true;
        for affordance in [
            A11yAffordance::Band,
            A11yAffordance::CornersGesture,
            A11yAffordance::DragFrom(A11yAnchor::SelectionCenter),
            A11yAffordance::Click(A11yAnchor::ObjectCenter(0)),
            A11yAffordance::MoveObject(0),
        ] {
            assert_eq!(
                affordance_commands(&live, ticket, affordance, "20,30,80,90"),
                Vec::new(),
                "{affordance:?} must be ignored while output owns the document"
            );
        }
    }

    #[test]
    fn a_corner_gesture_creates_an_object_inside_the_selection() {
        let ticket = ticket();
        let annotations: [Annotation; 0] = [];
        let live = context(Tool::Rectangle, Some(selection()), &annotations);
        assert_eq!(
            affordance_commands(
                &live,
                ticket,
                A11yAffordance::CornersGesture,
                "200,150,300,200"
            ),
            vec![
                UiCommand::PointerPressed {
                    ticket,
                    point: [200.0, 150.0],
                },
                UiCommand::PointerMoved {
                    ticket,
                    point: [300.0, 200.0],
                },
                UiCommand::PointerReleased(ticket),
            ]
        );
    }

    #[test]
    fn the_eight_handle_names_are_stable_and_distinct() {
        use arcshot_core::ResizeHandle;
        let names: Vec<&str> = [
            ResizeHandle::TopLeft,
            ResizeHandle::Top,
            ResizeHandle::TopRight,
            ResizeHandle::Right,
            ResizeHandle::BottomRight,
            ResizeHandle::Bottom,
            ResizeHandle::BottomLeft,
            ResizeHandle::Left,
        ]
        .into_iter()
        .map(handle_name)
        .collect();
        assert_eq!(
            names,
            vec![
                "top_left",
                "top",
                "top_right",
                "right",
                "bottom_right",
                "bottom",
                "bottom_left",
                "left",
            ]
        );
        let unique: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(unique.len(), 8, "handle ids must not collide");
    }

    #[test]
    fn the_two_endpoint_names_are_stable() {
        assert_eq!(endpoint_name(arcshot_core::Endpoint::Start), "start");
        assert_eq!(endpoint_name(arcshot_core::Endpoint::End), "end");
    }
}
