//! Annotation objects, their fixed sRGB rectangle style and topmost hit testing.
use crate::{document::ResizeHandle, document::Selection};

/// Half-extent of the logical hit area around each object control point.
/// It matches the 8-point visual handle and never exceeds a third of the object,
/// so small objects still offer a body move area.
const OBJECT_HANDLE_HIT_RADIUS: f64 = 4.0;

/// The pointer tool currently active in the frozen overlay.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tool {
    /// Select, move and resize the selection and existing objects.
    #[default]
    Select,
    /// Create rectangle objects by dragging inside the selection.
    Rectangle,
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

/// One of the three selectable rectangle stroke widths in logical points.
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

/// One rectangle annotation in display-local logical points.
///
/// Bounds may leave the selection; preview and output clip them per frame
/// instead of discarding the retained data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Annotation {
    bounds: Selection,
    style: RectangleStyle,
}

impl Annotation {
    /// Creates one rectangle object with its retained bounds and style.
    #[must_use]
    pub const fn new(bounds: Selection, style: RectangleStyle) -> Self {
        Self { bounds, style }
    }

    /// Returns the retained display-local geometry, even when it leaves the selection.
    #[must_use]
    pub const fn bounds(self) -> Selection {
        self.bounds
    }
    /// Returns the rectangle appearance.
    #[must_use]
    pub const fn style(self) -> RectangleStyle {
        self.style
    }

    /// Clips the retained geometry to one selection for preview or output.
    #[must_use]
    pub fn visible_bounds(self, selection: Selection) -> Option<Selection> {
        self.bounds.intersection(selection)
    }

    /// Returns a copy translated by a logical delta, rejecting non-finite results.
    #[must_use]
    pub fn translated(self, dx: f64, dy: f64) -> Option<Self> {
        Selection::try_new(
            self.bounds.x() + dx,
            self.bounds.y() + dy,
            self.bounds.width(),
            self.bounds.height(),
        )
        .ok()
        .map(|bounds| Self { bounds, ..self })
    }

    /// Returns a copy with one style option applied.
    #[must_use]
    pub const fn with_option(self, option: RectangleOption) -> Self {
        Self {
            style: self.style.with_option(option),
            ..self
        }
    }

    /// Returns the display-local center of one object resize control point.
    #[must_use]
    pub fn handle_center(self, handle: ResizeHandle) -> [f64; 2] {
        self.bounds.handle_center(handle)
    }

    /// Finds the object control point whose logical hit area contains `point`.
    #[must_use]
    pub fn resize_handle_at(self, point: [f64; 2]) -> Option<ResizeHandle> {
        let radius = OBJECT_HANDLE_HIT_RADIUS
            .min(self.bounds.width() / 3.0)
            .min(self.bounds.height() / 3.0);
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
            let center = self.handle_center(*handle);
            (point[0] - center[0]).abs() <= radius && (point[1] - center[1]).abs() <= radius
        })
    }
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
        .rposition(|annotation| annotation.bounds().contains(point))
        .or_else(|| {
            annotations
                .iter()
                .rposition(|annotation| annotation.resize_handle_at(point).is_some())
        })
}
