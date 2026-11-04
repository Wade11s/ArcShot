//! Annotation objects, their fixed sRGB styles, geometry and topmost hit testing.
use crate::{document::ResizeHandle, document::Selection};

/// Half-extent of the logical hit area around each object control point.
/// It matches the 8-point visual handle and never exceeds a third of the object,
/// so small objects still offer a body move area.
const OBJECT_HANDLE_HIT_RADIUS: f64 = 4.0;

/// Minimum logical distance from a thin line or arrow body that still selects it.
/// A two point stroke is one point away from its path, which is hard to hit.
const LINE_HIT_TOLERANCE: f64 = 3.0;

/// Flat arrow head length as a multiple of the stroke width, kept clear of the tip.
const ARROW_HEAD_LENGTH_PER_WIDTH: f64 = 3.0;
/// Flat arrow head half width as a multiple of the stroke width.
const ARROW_HEAD_HALF_WIDTH_PER_WIDTH: f64 = 1.6;
/// Flat arrow body half width as a multiple of the stroke width: constant body.
const ARROW_BODY_HALF_WIDTH_PER_WIDTH: f64 = 0.5;
/// Inward arrow tail half width as a multiple of the stroke width: a thin tail.
const ARROW_TAIL_HALF_WIDTH_PER_WIDTH: f64 = 0.15;
/// Inward arrow half width where the tapering shaft ends inside the head.
const ARROW_SHAFT_END_HALF_WIDTH_PER_WIDTH: f64 = 0.55;
/// Inward notch distance from the tip as a fraction of the head length.
const ARROW_NOTCH_FRACTION: f64 = 0.6;
/// The inward shaft ends this fraction of the notch distance in front of it,
/// so its cap stays hidden inside the solid part of the head.
const ARROW_SHAFT_END_FRACTION: f64 = 0.8;
/// Dashed line dash length as a multiple of the stroke width.
const DASH_LENGTH_PER_WIDTH: f64 = 3.0;
/// Dashed line gap length as a multiple of the stroke width.
const DASH_GAP_PER_WIDTH: f64 = 1.5;

/// The pointer tool currently active in the frozen overlay.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tool {
    /// Select, move and resize the selection and existing objects.
    #[default]
    Select,
    /// Create rectangle objects by dragging inside the selection.
    Rectangle,
    /// Create solid or dashed line objects by dragging inside the selection.
    Line,
    /// Create flat or inward arrow objects by dragging inside the selection.
    Arrow,
}

impl Tool {
    /// Whether this tool creates annotations instead of editing existing ones.
    #[must_use]
    pub const fn is_annotation(self) -> bool {
        !matches!(self, Self::Select)
    }
}

/// The opaque sRGB palette shared by every annotation tool.
///
/// The named values are the product contract; custom colours are not offered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PaletteColor {
    /// #FF3B30, the default.
    #[default]
    Red,
    /// #FF9500.
    Orange,
    /// #FFCC00.
    Yellow,
    /// #34C759.
    Green,
    /// #007AFF.
    Blue,
    /// #FFFFFF.
    White,
    /// #000000.
    Black,
}

impl PaletteColor {
    /// Every selectable colour in display order.
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::White,
        Self::Black,
    ];

    /// Opaque sRGB channels of this palette entry.
    #[must_use]
    pub const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [0xFF, 0x3B, 0x30],
            Self::Orange => [0xFF, 0x95, 0x00],
            Self::Yellow => [0xFF, 0xCC, 0x00],
            Self::Green => [0x34, 0xC7, 0x59],
            Self::Blue => [0x00, 0x7A, 0xFF],
            Self::White => [0xFF, 0xFF, 0xFF],
            Self::Black => [0x00, 0x00, 0x00],
        }
    }

    /// Stable uppercase hex form used by text diagnostics and tooltips.
    #[must_use]
    pub const fn hex(self) -> &'static str {
        match self {
            Self::Red => "#FF3B30",
            Self::Orange => "#FF9500",
            Self::Yellow => "#FFCC00",
            Self::Green => "#34C759",
            Self::Blue => "#007AFF",
            Self::White => "#FFFFFF",
            Self::Black => "#000000",
        }
    }
}

/// One of the three selectable stroke widths in logical points.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StrokeWidth {
    /// 2 logical points.
    Two,
    /// 3 logical points, the default.
    #[default]
    Three,
    /// 5 logical points.
    Five,
}

impl StrokeWidth {
    /// Every selectable width in display order.
    pub const ALL: [Self; 3] = [Self::Two, Self::Three, Self::Five];

    /// Stroke width in logical points.
    #[must_use]
    pub const fn points(self) -> f64 {
        match self {
            Self::Two => 2.0,
            Self::Three => 3.0,
            Self::Five => 5.0,
        }
    }

    /// Stable label used by tooltips and text diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Two => "2",
            Self::Three => "3",
            Self::Five => "5",
        }
    }
}

/// The complete rectangle appearance: stroke colour, width and same-colour fill.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RectangleStyle {
    stroke: PaletteColor,
    width: StrokeWidth,
    fill: bool,
}

impl RectangleStyle {
    /// Builds one style from its three independent options.
    #[must_use]
    pub const fn new(stroke: PaletteColor, width: StrokeWidth, fill: bool) -> Self {
        Self {
            stroke,
            width,
            fill,
        }
    }

    /// Stroke colour.
    #[must_use]
    pub const fn stroke(self) -> PaletteColor {
        self.stroke
    }
    /// Stroke width in logical points.
    #[must_use]
    pub const fn width(self) -> StrokeWidth {
        self.width
    }
    /// Whether the object is filled with the same colour at 20% opacity.
    #[must_use]
    pub const fn fill(self) -> bool {
        self.fill
    }

    /// Applies one option, returning the resulting style.
    #[must_use]
    pub const fn with_option(self, option: RectangleOption) -> Self {
        match option {
            RectangleOption::Stroke(stroke) => Self { stroke, ..self },
            RectangleOption::Width(width) => Self { width, ..self },
            RectangleOption::Fill(fill) => Self { fill, ..self },
        }
    }
}

/// One selectable rectangle option; clicking it updates the tool memory and,
/// when a rectangle object is selected, that object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RectangleOption {
    /// Stroke colour from the fixed palette.
    Stroke(PaletteColor),
    /// Stroke width in logical points.
    Width(StrokeWidth),
    /// Same-colour 20% opacity fill.
    Fill(bool),
}

/// The line appearance: stroke colour, width and solid/dashed pattern.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineStyle {
    stroke: PaletteColor,
    width: StrokeWidth,
    dash: bool,
}

impl LineStyle {
    /// Builds one line style from its three independent options.
    #[must_use]
    pub const fn new(stroke: PaletteColor, width: StrokeWidth, dash: bool) -> Self {
        Self {
            stroke,
            width,
            dash,
        }
    }

    /// Stroke colour.
    #[must_use]
    pub const fn stroke(self) -> PaletteColor {
        self.stroke
    }
    /// Stroke width in logical points.
    #[must_use]
    pub const fn width(self) -> StrokeWidth {
        self.width
    }
    /// Whether the line is dashed instead of solid.
    #[must_use]
    pub const fn dash(self) -> bool {
        self.dash
    }

    /// Applies one option, returning the resulting style.
    #[must_use]
    pub const fn with_option(self, option: LineOption) -> Self {
        match option {
            LineOption::Stroke(stroke) => Self { stroke, ..self },
            LineOption::Width(width) => Self { width, ..self },
            LineOption::Dash(dash) => Self { dash, ..self },
        }
    }

    /// Logical dash and gap lengths; both scale with the stroke width.
    #[must_use]
    pub fn dash_pattern(self) -> (f64, f64) {
        let points = self.width.points();
        (DASH_LENGTH_PER_WIDTH * points, DASH_GAP_PER_WIDTH * points)
    }
}

/// One selectable line option; clicking it updates the line tool memory and,
/// when a line object is selected, that object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineOption {
    /// Stroke colour from the fixed palette.
    Stroke(PaletteColor),
    /// Stroke width in logical points.
    Width(StrokeWidth),
    /// Solid or dashed pattern.
    Dash(bool),
}

/// The arrow appearance: stroke colour, width and Flat/Inward head shape.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArrowStyle {
    stroke: PaletteColor,
    width: StrokeWidth,
    inward: bool,
}

impl ArrowStyle {
    /// Builds one arrow style from its three independent options.
    #[must_use]
    pub const fn new(stroke: PaletteColor, width: StrokeWidth, inward: bool) -> Self {
        Self {
            stroke,
            width,
            inward,
        }
    }

    /// Stroke colour.
    #[must_use]
    pub const fn stroke(self) -> PaletteColor {
        self.stroke
    }
    /// Stroke width in logical points.
    #[must_use]
    pub const fn width(self) -> StrokeWidth {
        self.width
    }
    /// Whether the arrow uses the inward instead of the flat head.
    #[must_use]
    pub const fn inward(self) -> bool {
        self.inward
    }

    /// Applies one option, returning the resulting style.
    #[must_use]
    pub const fn with_option(self, option: ArrowOption) -> Self {
        match option {
            ArrowOption::Stroke(stroke) => Self { stroke, ..self },
            ArrowOption::Width(width) => Self { width, ..self },
            ArrowOption::Inward(inward) => Self { inward, ..self },
        }
    }
}

/// One selectable arrow option; clicking it updates the arrow tool memory and,
/// when an arrow object is selected, that object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArrowOption {
    /// Stroke colour from the fixed palette.
    Stroke(PaletteColor),
    /// Stroke width in logical points.
    Width(StrokeWidth),
    /// Flat or inward head shape.
    Inward(bool),
}

/// The appearance of one annotation, tagged by the tool that owns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnnotationStyle {
    /// Rectangle stroke and optional fill.
    Rectangle(RectangleStyle),
    /// Line stroke and solid/dashed pattern.
    Line(LineStyle),
    /// Arrow stroke and Flat/Inward head shape.
    Arrow(ArrowStyle),
}

impl AnnotationStyle {
    /// The tool whose objects use this style.
    #[must_use]
    pub const fn tool(self) -> Tool {
        match self {
            Self::Rectangle(_) => Tool::Rectangle,
            Self::Line(_) => Tool::Line,
            Self::Arrow(_) => Tool::Arrow,
        }
    }

    /// Stroke colour shared by every tool.
    #[must_use]
    pub const fn stroke(self) -> PaletteColor {
        match self {
            Self::Rectangle(style) => style.stroke(),
            Self::Line(style) => style.stroke(),
            Self::Arrow(style) => style.stroke(),
        }
    }

    /// Stroke width in logical points shared by every tool.
    #[must_use]
    pub const fn width(self) -> StrokeWidth {
        match self {
            Self::Rectangle(style) => style.width(),
            Self::Line(style) => style.width(),
            Self::Arrow(style) => style.width(),
        }
    }

    /// The rectangle appearance when this is a rectangle style.
    #[must_use]
    pub const fn rectangle(self) -> Option<RectangleStyle> {
        match self {
            Self::Rectangle(style) => Some(style),
            Self::Line(_) | Self::Arrow(_) => None,
        }
    }

    /// The line appearance when this is a line style.
    #[must_use]
    pub const fn line(self) -> Option<LineStyle> {
        match self {
            Self::Line(style) => Some(style),
            Self::Rectangle(_) | Self::Arrow(_) => None,
        }
    }

    /// The arrow appearance when this is an arrow style.
    #[must_use]
    pub const fn arrow(self) -> Option<ArrowStyle> {
        match self {
            Self::Arrow(style) => Some(style),
            Self::Rectangle(_) | Self::Line(_) => None,
        }
    }
}

/// The start (`tail`) or end (`tip`) of a line or arrow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endpoint {
    /// Start of the stroke, the tail the arrow points away from.
    Start,
    /// End of the stroke, the arrow tip.
    End,
}

/// A directed segment in display-local logical points.
///
/// The direction always runs from the start tail to the end tip, so an arrow
/// never flips when its endpoints are dragged past each other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    start: [f64; 2],
    end: [f64; 2],
}

impl Segment {
    /// Validates finite endpoints; coincident endpoints are allowed for edits.
    ///
    /// # Errors
    /// Returns [`crate::DocumentError::Geometry`] for non-finite coordinates.
    pub fn try_new(start: [f64; 2], end: [f64; 2]) -> Result<Self, crate::DocumentError> {
        if ![start[0], start[1], end[0], end[1]]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(crate::DocumentError::Geometry);
        }
        Ok(Self { start, end })
    }

    /// Builds the segment of one creation drag, rejecting a zero-length stroke.
    ///
    /// # Errors
    /// Returns [`crate::DocumentError::Empty`] for coincident endpoints and
    /// [`crate::DocumentError::Geometry`] for non-finite coordinates.
    pub fn from_drag(start: [f64; 2], end: [f64; 2]) -> Result<Self, crate::DocumentError> {
        let segment = Self::try_new(start, end)?;
        if segment.is_zero() {
            return Err(crate::DocumentError::Empty);
        }
        Ok(segment)
    }

    /// Start point, the arrow tail.
    #[must_use]
    pub const fn start(self) -> [f64; 2] {
        self.start
    }
    /// End point, the arrow tip.
    #[must_use]
    pub const fn end(self) -> [f64; 2] {
        self.end
    }

    /// Whether both endpoints coincide.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.start == self.end
    }

    /// Euclidean length in logical points.
    #[must_use]
    pub fn length(self) -> f64 {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        dx.hypot(dy)
    }

    /// Unit direction from start to end, or `None` for a coincident segment.
    #[must_use]
    fn direction(self) -> Option<[f64; 2]> {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let length = dx.hypot(dy);
        if length == 0.0 {
            return None;
        }
        Some([dx / length, dy / length])
    }

    /// Returns a copy translated by a logical delta, rejecting non-finite results.
    #[must_use]
    pub fn translated(self, dx: f64, dy: f64) -> Option<Self> {
        Self::try_new(
            [self.start[0] + dx, self.start[1] + dy],
            [self.end[0] + dx, self.end[1] + dy],
        )
        .ok()
    }

    /// Returns a copy with one endpoint moved; endpoints may coincide.
    #[must_use]
    pub fn with_endpoint(self, endpoint: Endpoint, point: [f64; 2]) -> Option<Self> {
        match endpoint {
            Endpoint::Start => Self::try_new(point, self.end).ok(),
            Endpoint::End => Self::try_new(self.start, point).ok(),
        }
    }

    /// Display-local center of one endpoint control point.
    #[must_use]
    pub fn endpoint_center(self, endpoint: Endpoint) -> [f64; 2] {
        match endpoint {
            Endpoint::Start => self.start,
            Endpoint::End => self.end,
        }
    }

    /// Finds the endpoint whose logical hit area contains `point`, nearest first.
    fn endpoint_at(self, point: [f64; 2]) -> Option<Endpoint> {
        let distance = |endpoint: Endpoint| {
            let center = self.endpoint_center(endpoint);
            (point[0] - center[0]).hypot(point[1] - center[1])
        };
        let start = distance(Endpoint::Start);
        let end = distance(Endpoint::End);
        let (nearest, distance) = if start <= end {
            (Endpoint::Start, start)
        } else {
            (Endpoint::End, end)
        };
        (distance <= OBJECT_HANDLE_HIT_RADIUS).then_some(nearest)
    }

    /// Shortest logical distance from a point to the segment.
    #[must_use]
    pub fn distance_to(self, point: [f64; 2]) -> f64 {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let length_squared = dx * dx + dy * dy;
        if length_squared == 0.0 {
            return (point[0] - self.start[0]).hypot(point[1] - self.start[1]);
        }
        let projection = (((point[0] - self.start[0]) * dx + (point[1] - self.start[1]) * dy)
            / length_squared)
            .clamp(0.0, 1.0);
        let closest = [
            self.start[0] + projection * dx,
            self.start[1] + projection * dy,
        ];
        (point[0] - closest[0]).hypot(point[1] - closest[1])
    }
}

/// The retained geometry of one annotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A rectangle, possibly leaving the selection.
    Rectangle(Selection),
    /// A directed line or arrow segment, possibly leaving the selection.
    Line(Segment),
}

impl Shape {
    /// The rectangle bounds when this shape is a rectangle.
    #[must_use]
    pub const fn rectangle_bounds(self) -> Option<Selection> {
        match self {
            Self::Rectangle(bounds) => Some(bounds),
            Self::Line(_) => None,
        }
    }

    /// The directed segment when this shape is a line or arrow.
    #[must_use]
    pub const fn segment(self) -> Option<Segment> {
        match self {
            Self::Line(segment) => Some(segment),
            Self::Rectangle(_) => None,
        }
    }

    /// Returns a copy translated by a logical delta, rejecting non-finite results.
    #[must_use]
    pub fn translated(self, dx: f64, dy: f64) -> Option<Self> {
        match self {
            Self::Rectangle(bounds) => Selection::try_new(
                bounds.x() + dx,
                bounds.y() + dy,
                bounds.width(),
                bounds.height(),
            )
            .ok()
            .map(Self::Rectangle),
            Self::Line(segment) => segment.translated(dx, dy).map(Self::Line),
        }
    }
}

/// One annotation in display-local logical points.
///
/// Bounds and segments may leave the selection; preview and output clip them
/// per frame instead of discarding the retained data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Annotation {
    shape: Shape,
    style: AnnotationStyle,
}

impl Annotation {
    /// Creates one rectangle object with its retained bounds and style.
    #[must_use]
    pub const fn rectangle(bounds: Selection, style: RectangleStyle) -> Self {
        Self {
            shape: Shape::Rectangle(bounds),
            style: AnnotationStyle::Rectangle(style),
        }
    }

    /// Creates one line object with its directed segment and style.
    #[must_use]
    pub const fn line(segment: Segment, style: LineStyle) -> Self {
        Self {
            shape: Shape::Line(segment),
            style: AnnotationStyle::Line(style),
        }
    }

    /// Creates one arrow object with its directed segment and style.
    #[must_use]
    pub const fn arrow(segment: Segment, style: ArrowStyle) -> Self {
        Self {
            shape: Shape::Line(segment),
            style: AnnotationStyle::Arrow(style),
        }
    }

    /// Returns the retained display-local geometry.
    #[must_use]
    pub const fn shape(self) -> Shape {
        self.shape
    }

    /// Returns the appearance tagged by its owning tool.
    #[must_use]
    pub const fn style(self) -> AnnotationStyle {
        self.style
    }

    /// Clips a retained rectangle to one selection for preview or output.
    #[must_use]
    pub fn visible_rectangle(self, selection: Selection) -> Option<Selection> {
        self.shape
            .rectangle_bounds()
            .and_then(|bounds| bounds.intersection(selection))
    }

    /// Returns a copy translated by a logical delta, rejecting non-finite results.
    #[must_use]
    pub fn translated(self, dx: f64, dy: f64) -> Option<Self> {
        self.shape
            .translated(dx, dy)
            .map(|shape| Self { shape, ..self })
    }

    /// Returns a copy with one rectangle option applied, or `None` for other types.
    #[must_use]
    pub const fn with_rectangle_option(self, option: RectangleOption) -> Option<Self> {
        match self.style {
            AnnotationStyle::Rectangle(style) => Some(Self {
                style: AnnotationStyle::Rectangle(style.with_option(option)),
                ..self
            }),
            AnnotationStyle::Line(_) | AnnotationStyle::Arrow(_) => None,
        }
    }

    /// Returns a copy with one line option applied, or `None` for other types.
    #[must_use]
    pub const fn with_line_option(self, option: LineOption) -> Option<Self> {
        match self.style {
            AnnotationStyle::Line(style) => Some(Self {
                style: AnnotationStyle::Line(style.with_option(option)),
                ..self
            }),
            AnnotationStyle::Rectangle(_) | AnnotationStyle::Arrow(_) => None,
        }
    }

    /// Returns a copy with one arrow option applied, or `None` for other types.
    #[must_use]
    pub const fn with_arrow_option(self, option: ArrowOption) -> Option<Self> {
        match self.style {
            AnnotationStyle::Arrow(style) => Some(Self {
                style: AnnotationStyle::Arrow(style.with_option(option)),
                ..self
            }),
            AnnotationStyle::Rectangle(_) | AnnotationStyle::Line(_) => None,
        }
    }

    /// Returns a copy with one endpoint moved; only line and arrow shapes have
    /// endpoints, and the two may coincide without deleting the object.
    #[must_use]
    pub fn with_endpoint(self, endpoint: Endpoint, point: [f64; 2]) -> Option<Self> {
        let segment = self.shape.segment()?;
        segment.with_endpoint(endpoint, point).map(|segment| Self {
            shape: Shape::Line(segment),
            ..self
        })
    }

    /// Returns a copy with a new rectangle shape, keeping its style.
    #[must_use]
    pub const fn with_rectangle_bounds(self, bounds: Selection) -> Option<Self> {
        match self.shape {
            Shape::Rectangle(_) => Some(Self {
                shape: Shape::Rectangle(bounds),
                ..self
            }),
            Shape::Line(_) => None,
        }
    }

    /// Returns the display-local center of one rectangle control point.
    #[must_use]
    pub fn handle_center(self, handle: ResizeHandle) -> Option<[f64; 2]> {
        self.shape
            .rectangle_bounds()
            .map(|bounds| bounds.handle_center(handle))
    }

    /// Finds the rectangle control point whose logical hit area contains `point`.
    #[must_use]
    pub fn resize_handle_at(self, point: [f64; 2]) -> Option<ResizeHandle> {
        let bounds = self.shape.rectangle_bounds()?;
        let radius = OBJECT_HANDLE_HIT_RADIUS
            .min(bounds.width() / 3.0)
            .min(bounds.height() / 3.0);
        const ORDER: [ResizeHandle; 8] = [
            ResizeHandle::TopLeft,
            ResizeHandle::TopRight,
            ResizeHandle::BottomRight,
            ResizeHandle::BottomLeft,
            ResizeHandle::Top,
            ResizeHandle::Right,
            ResizeHandle::Bottom,
            ResizeHandle::Left,
        ];
        ORDER.into_iter().find(|handle| {
            let center = bounds.handle_center(*handle);
            (point[0] - center[0]).abs() <= radius && (point[1] - center[1]).abs() <= radius
        })
    }

    /// Finds the line or arrow endpoint whose logical hit area contains `point`.
    #[must_use]
    pub fn endpoint_at(self, point: [f64; 2]) -> Option<Endpoint> {
        self.shape.segment()?.endpoint_at(point)
    }

    /// Whether a display-local point hits this object's body.
    ///
    /// A rectangle body is its filled area. A line is selectable within a small
    /// tolerance around its path, and an arrow's head is part of its body.
    #[must_use]
    pub fn contains(self, point: [f64; 2]) -> bool {
        match self.shape {
            Shape::Rectangle(bounds) => bounds.contains(point),
            Shape::Line(segment) => {
                let half = self.style.width().points() / 2.0;
                if segment.distance_to(point) <= half.max(LINE_HIT_TOLERANCE) {
                    return true;
                }
                self.arrow_outlines().is_some_and(|outlines| {
                    outlines
                        .iter()
                        .any(|outline| polygon_contains(outline, point))
                })
            }
        }
    }

    /// The filled outlines of an arrow in logical points, or `None` for other types.
    ///
    /// The arrow is the union of its head and shaft, returned as two simple
    /// polygons: a dart-shaped head with a concave rear edge and the tapering
    /// shaft that pokes into the notch. The outlines are shared by hit testing
    /// and the image rasterizer so the clickable and drawn shapes cannot drift.
    /// The direction always runs from the start tail to the end tip; a
    /// coincident segment has no outline and draws nothing.
    #[must_use]
    pub fn arrow_outlines(self) -> Option<Vec<Vec<[f64; 2]>>> {
        let AnnotationStyle::Arrow(style) = self.style else {
            return None;
        };
        let segment = self.shape.segment()?;
        let direction = segment.direction()?;
        let perpendicular = [-direction[1], direction[0]];
        let length = segment.length();
        let width = style.width().points();
        let head_length = (ARROW_HEAD_LENGTH_PER_WIDTH * width).min(length);
        let head_half = ARROW_HEAD_HALF_WIDTH_PER_WIDTH * width;
        let base = advance(segment.end, direction, -head_length);
        let corner = |origin: [f64; 2], half: f64, sign: f64| {
            [
                origin[0] + perpendicular[0] * half * sign,
                origin[1] + perpendicular[1] * half * sign,
            ]
        };
        if style.inward() {
            let notch_length = head_length * ARROW_NOTCH_FRACTION;
            let notch = advance(segment.end, direction, -notch_length);
            let shaft_end = advance(
                segment.end,
                direction,
                -notch_length * ARROW_SHAFT_END_FRACTION,
            );
            let tail_half = ARROW_TAIL_HALF_WIDTH_PER_WIDTH * width;
            let shaft_half = ARROW_SHAFT_END_HALF_WIDTH_PER_WIDTH * width;
            let shaft = vec![
                corner(segment.start, tail_half, 1.0),
                corner(shaft_end, shaft_half, 1.0),
                corner(shaft_end, shaft_half, -1.0),
                corner(segment.start, tail_half, -1.0),
            ];
            let head = vec![
                segment.end,
                corner(base, head_half, 1.0),
                notch,
                corner(base, head_half, -1.0),
            ];
            return Some(vec![shaft, head]);
        }
        let body_half = ARROW_BODY_HALF_WIDTH_PER_WIDTH * width;
        Some(vec![vec![
            corner(segment.start, body_half, 1.0),
            corner(base, body_half, 1.0),
            corner(base, head_half, 1.0),
            segment.end,
            corner(base, head_half, -1.0),
            corner(base, body_half, -1.0),
            corner(segment.start, body_half, -1.0),
        ]])
    }
}

/// Moves a point along one unit direction by `distance` logical points.
fn advance(origin: [f64; 2], direction: [f64; 2], distance: f64) -> [f64; 2] {
    [
        origin[0] + direction[0] * distance,
        origin[1] + direction[1] * distance,
    ]
}

/// Crossing-number point-in-polygon test for the arrow silhouette.
fn polygon_contains(polygon: &[[f64; 2]], point: [f64; 2]) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        let (x0, y0) = (previous[0], previous[1]);
        let (x1, y1) = (current[0], current[1]);
        if (y0 > point[1]) != (y1 > point[1]) {
            let crossing = x0 + (point[1] - y0) * (x1 - x0) / (y1 - y0);
            if point[0] < crossing {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

/// Finds the topmost object at a display-local point.
///
/// Layers are stored bottom-to-top, so the last match is the one the user sees.
/// A body that contains the point always wins over a neighbouring object's
/// control-point hit area; handles alone are considered only when no body does.
#[must_use]
pub fn annotation_at(annotations: &[Annotation], point: [f64; 2]) -> Option<usize> {
    annotations
        .iter()
        .rposition(|annotation| annotation.contains(point))
        .or_else(|| {
            annotations.iter().rposition(|annotation| {
                annotation.resize_handle_at(point).is_some()
                    || annotation.endpoint_at(point).is_some()
            })
        })
}
