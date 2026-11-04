//! Immutable capture document, logical region selection and physical output geometry.
use crate::{CapturedFrame, ScreenGeometry, frame::MAX_EDGE, frame::MAX_PIXELS};
use std::{error::Error, fmt};

/// V1 maximum encoded PNG size, checked before any clipboard or file side effect.
pub const MAX_PNG_BYTES: usize = 128 * 1024 * 1024;

/// A non-empty region in display-local logical points, before physical conversion.
///
/// The selection is not yet proven to lie on the captured screen; [`Document`]
/// validates it against the admitted frame and its 2x scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Selection {
    /// Validates finite, positive logical extents.
    ///
    /// # Errors
    /// Returns [`DocumentError::Geometry`] for non-finite or non-positive extents.
    pub fn try_new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, DocumentError> {
        if ![x, y, width, height].iter().all(|v| v.is_finite()) || width <= 0.0 || height <= 0.0 {
            return Err(DocumentError::Geometry);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// Builds a normalized selection from a pointer drag, rejecting a zero-size drag.
    ///
    /// # Errors
    /// Returns [`DocumentError::Empty`] when either extent is zero, and
    /// [`DocumentError::Geometry`] for non-finite input.
    pub fn from_drag(start: [f64; 2], end: [f64; 2]) -> Result<Self, DocumentError> {
        if ![start[0], start[1], end[0], end[1]]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(DocumentError::Geometry);
        }
        let x = start[0].min(end[0]);
        let y = start[1].min(end[1]);
        let width = (end[0] - start[0]).abs();
        let height = (end[1] - start[1]).abs();
        if width == 0.0 || height == 0.0 {
            return Err(DocumentError::Empty);
        }
        Self::try_new(x, y, width, height)
    }

    /// Logical left edge in display-local points.
    #[must_use]
    pub const fn x(self) -> f64 {
        self.x
    }
    /// Logical top edge in display-local points.
    #[must_use]
    pub const fn y(self) -> f64 {
        self.y
    }
    /// Logical width in points.
    #[must_use]
    pub const fn width(self) -> f64 {
        self.width
    }
    /// Logical height in points.
    #[must_use]
    pub const fn height(self) -> f64 {
        self.height
    }

    /// Whether a display-local point lies inside or on the selection edge.
    #[must_use]
    pub fn contains(self, point: [f64; 2]) -> bool {
        point[0] >= self.x
            && point[0] <= self.x + self.width
            && point[1] >= self.y
            && point[1] <= self.y + self.height
    }

    /// Converts the single logical rectangle directly to physical output pixels.
    ///
    /// Each edge is rounded exactly once from logical coordinates; view scale or
    /// position never participates. The result is bounded by the captured screen,
    /// V1's edge limit and V1's document pixel budget.
    ///
    /// # Errors
    /// Returns [`DocumentError::OutOfBounds`] when the region leaves the captured
    /// screen, [`DocumentError::Empty`] when rounding removes an extent, and
    /// [`DocumentError::Dimensions`] when the edge/pixel limit is exceeded.
    pub fn pixel_rect(self, screen: ScreenGeometry) -> Result<PixelRect, DocumentError> {
        let [logical_width, logical_height] = screen.source_rect()[2..]
            .try_into()
            .map_err(|_| DocumentError::Geometry)?;
        const EPSILON: f64 = 1e-6;
        if self.x < 0.0
            || self.y < 0.0
            || self.x + self.width > logical_width + EPSILON
            || self.y + self.height > logical_height + EPSILON
        {
            return Err(DocumentError::OutOfBounds);
        }
        let scale = screen.scale();
        let left = (self.x * scale).round();
        let top = (self.y * scale).round();
        let right = ((self.x + self.width) * scale).round();
        let bottom = ((self.y + self.height) * scale).round();
        let [screen_width, screen_height] = screen.pixel_size();
        if right > f64::from(screen_width) || bottom > f64::from(screen_height) {
            return Err(DocumentError::OutOfBounds);
        }
        if right <= left || bottom <= top {
            return Err(DocumentError::Empty);
        }
        PixelRect::try_new(
            left as u32,
            top as u32,
            (right - left) as u32,
            (bottom - top) as u32,
        )
    }
}

/// A non-empty physical pixel rectangle inside the captured frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    /// Validates a physical rectangle against V1's edge and pixel limits.
    ///
    /// # Errors
    /// Returns [`DocumentError::Dimensions`] when empty, too long or over budget.
    pub fn try_new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, DocumentError> {
        if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
            return Err(DocumentError::Dimensions);
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)
            .ok_or(DocumentError::Dimensions)?;
        if pixels > MAX_PIXELS {
            return Err(DocumentError::Dimensions);
        }
        x.checked_add(width).ok_or(DocumentError::Dimensions)?;
        y.checked_add(height).ok_or(DocumentError::Dimensions)?;
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
    /// Left edge in physical pixels.
    #[must_use]
    pub const fn x(self) -> u32 {
        self.x
    }
    /// Top edge in physical pixels.
    #[must_use]
    pub const fn y(self) -> u32 {
        self.y
    }
    /// Width in physical pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }
    /// Height in physical pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
    /// Total physical pixels.
    #[must_use]
    pub fn pixel_count(self) -> usize {
        self.width as usize * self.height as usize
    }
}

/// The single immutable-base document shared by region and scroll output.
///
/// The base image is never copied for selection changes; a version increments
/// only when the selection actually changes, so output can be fenced.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    frame: CapturedFrame,
    selection: Option<Selection>,
    version: u64,
}

impl Document {
    /// Creates a frozen document with no selection.
    #[must_use]
    pub const fn new(frame: CapturedFrame) -> Self {
        Self {
            frame,
            selection: None,
            version: 0,
        }
    }

    /// Borrows the immutable base image.
    #[must_use]
    pub const fn frame(&self) -> &CapturedFrame {
        &self.frame
    }
    /// Returns the current selection, if any.
    #[must_use]
    pub const fn selection(&self) -> Option<Selection> {
        self.selection
    }
    /// Returns the version used by output fences.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Replaces the selection after validating its physical output rectangle.
    ///
    /// Selecting the same rectangle again is not a change and does not bump the
    /// version. A rejected rectangle leaves the existing selection untouched.
    ///
    /// # Errors
    /// Returns the [`Selection::pixel_rect`] error when the region is outside the
    /// captured screen, empty after 2x rounding, or over the output limits.
    pub fn select(&mut self, selection: Selection) -> Result<(), DocumentError> {
        selection.pixel_rect(self.frame.geometry())?;
        if self.selection == Some(selection) {
            return Ok(());
        }
        let version = self
            .version
            .checked_add(1)
            .ok_or(DocumentError::VersionExhausted)?;
        self.selection = Some(selection);
        self.version = version;
        Ok(())
    }

    /// Converts the current selection to the physical crop used by image output.
    ///
    /// # Errors
    /// Returns [`DocumentError::NoSelection`] when nothing has been selected.
    pub fn crop(&self) -> Result<PixelRect, DocumentError> {
        self.selection
            .ok_or(DocumentError::NoSelection)?
            .pixel_rect(self.frame.geometry())
    }
}

/// Region geometry rejected before it can become a document selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentError {
    /// Selection coordinates are non-finite or non-positive.
    Geometry,
    /// The region leaves the captured screen.
    OutOfBounds,
    /// The region has no physical extent after 2x rounding.
    Empty,
    /// The region exceeds V1's edge or pixel limit.
    Dimensions,
    /// An output was requested without a selection.
    NoSelection,
    /// The document version counter cannot advance.
    VersionExhausted,
}
impl fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Geometry => "invalid selection geometry",
            Self::OutOfBounds => "selection is outside the captured screen",
            Self::Empty => "selection has no physical extent",
            Self::Dimensions => "selection exceeds output limits",
            Self::NoSelection => "document has no selection",
            Self::VersionExhausted => "document version exhausted",
        })
    }
}
impl Error for DocumentError {}
