//! Immutable capture document, logical region selection and physical output geometry.
use crate::{CapturedFrame, ScreenGeometry, frame::MAX_EDGE, frame::MAX_PIXELS};
use std::{error::Error, fmt};

/// V1 maximum encoded PNG size, checked before any clipboard or file side effect.
pub const MAX_PNG_BYTES: usize = 128 * 1024 * 1024;

/// Smallest selection extent in logical points, one physical pixel at V1's 2x scale.
const MIN_SELECTION_POINTS: f64 = 0.5;
/// Half-extent of the logical hit area around each resize control point.
/// It matches the 8-point visual handle and never exceeds a third of the
/// selection, so tiny selections still offer a move area.
const RESIZE_HANDLE_HIT_RADIUS: f64 = 4.0;

/// One of the eight control points used to resize the active selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeHandle {
    /// Top-left corner.
    TopLeft,
    /// Top edge midpoint.
    Top,
    /// Top-right corner.
    TopRight,
    /// Right edge midpoint.
    Right,
    /// Bottom-right corner.
    BottomRight,
    /// Bottom edge midpoint.
    Bottom,
    /// Bottom-left corner.
    BottomLeft,
    /// Left edge midpoint.
    Left,
}

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

    /// Intersects this rectangle with another, returning `None` when empty.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        if right <= left || bottom <= top {
            return None;
        }
        Self::try_new(left, top, right - left, bottom - top).ok()
    }

    /// Returns the display-local center of one resize control point.
    #[must_use]
    pub fn handle_center(self, handle: ResizeHandle) -> [f64; 2] {
        let (left, right) = (self.x, self.x + self.width);
        let (top, bottom) = (self.y, self.y + self.height);
        match handle {
            ResizeHandle::TopLeft => [left, top],
            ResizeHandle::Top => [(left + right) / 2.0, top],
            ResizeHandle::TopRight => [right, top],
            ResizeHandle::Right => [right, (top + bottom) / 2.0],
            ResizeHandle::BottomRight => [right, bottom],
            ResizeHandle::Bottom => [(left + right) / 2.0, bottom],
            ResizeHandle::BottomLeft => [left, bottom],
            ResizeHandle::Left => [left, (top + bottom) / 2.0],
        }
    }

    /// Finds the resize control point whose logical hit area contains `point`.
    ///
    /// Corners take priority over edge midpoints; the within-selection move test
    /// therefore only runs when no control point was hit.
    #[must_use]
    pub fn resize_handle_at(self, point: [f64; 2]) -> Option<ResizeHandle> {
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
        let radius = RESIZE_HANDLE_HIT_RADIUS
            .min(self.width / 3.0)
            .min(self.height / 3.0);
        ORDER.into_iter().find(|handle| {
            let center = self.handle_center(*handle);
            (point[0] - center[0]).abs() <= radius && (point[1] - center[1]).abs() <= radius
        })
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

/// The logical footprint of one layer-ordered annotation.
///
/// This slice carries geometry only so moving or resizing the selection already
/// has defined annotation semantics; drawing tools and their styling arrive with
/// the first object slice. Retained bounds may leave the selection; preview and
/// output clip them per frame instead of discarding the data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Annotation {
    bounds: Selection,
}

impl Annotation {
    const fn from_bounds(bounds: Selection) -> Self {
        Self { bounds }
    }
    /// Returns the retained display-local geometry, even when it leaves the selection.
    #[must_use]
    pub const fn bounds(self) -> Selection {
        self.bounds
    }
    /// Clips the retained geometry to one selection for preview or output.
    #[must_use]
    pub fn visible_bounds(self, selection: Selection) -> Option<Selection> {
        self.bounds.intersection(selection)
    }
    fn translated(self, dx: f64, dy: f64) -> Option<Self> {
        Selection::try_new(
            self.bounds.x() + dx,
            self.bounds.y() + dy,
            self.bounds.width(),
            self.bounds.height(),
        )
        .ok()
        .map(Self::from_bounds)
    }
}

/// The edit a pointer drag performs on the single active selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DragTarget {
    /// Dragging inside the selection moves it and every annotation.
    Move,
    /// Dragging one control point resizes the selection only.
    Resize(ResizeHandle),
}

/// A committed document change, kept as edit state so the base image is shared.
#[derive(Clone, Debug, PartialEq)]
struct EditState {
    selection: Option<Selection>,
    annotations: Vec<Annotation>,
}

/// One pointer drag: its target, origin and the state it may commit at release.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    target: DragTarget,
    start: [f64; 2],
    baseline: EditState,
}

/// The single immutable-base document shared by region and scroll output.
///
/// The base image is never copied for selection or annotation edits. History
/// snapshots hold edit state only; a version increments once per committed
/// change so output can be fenced.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    frame: CapturedFrame,
    selection: Option<Selection>,
    /// Bottom-to-top layer order: later entries draw above earlier ones.
    annotations: Vec<Annotation>,
    history: History,
    drag: Option<Drag>,
    version: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct History {
    undo: Vec<EditState>,
    redo: Vec<EditState>,
}

impl Document {
    /// Creates a frozen document with no selection and no history.
    #[must_use]
    pub const fn new(frame: CapturedFrame) -> Self {
        Self {
            frame,
            selection: None,
            annotations: Vec::new(),
            history: History {
                undo: Vec::new(),
                redo: Vec::new(),
            },
            drag: None,
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
    /// Borrows annotations in bottom-to-top layer order.
    #[must_use]
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Adds one annotation above the existing layers as a single undoable step.
    ///
    /// Retained bounds may leave the selection; clipping happens per preview or
    /// output frame via [`Annotation::visible_bounds`].
    ///
    /// # Errors
    /// Returns [`DocumentError::NoSelection`] without a single active selection and
    /// [`DocumentError::VersionExhausted`] when the history version cannot advance.
    pub fn add_annotation(&mut self, bounds: Selection) -> Result<(), DocumentError> {
        if self.selection.is_none() {
            return Err(DocumentError::NoSelection);
        }
        let version = self.next_version()?;
        let previous = self.edit_state();
        self.annotations.push(Annotation::from_bounds(bounds));
        self.commit(previous, version);
        Ok(())
    }

    /// Starts one selection drag at a display-local point.
    ///
    /// Only the select tool exists in this slice, so a press inside the single
    /// selection starts moving it and all annotations. A press outside starts no
    /// core drag; the overlay treats it as a new selection band.
    pub fn begin_drag(&mut self, point: [f64; 2]) -> Option<DragTarget> {
        if self.drag.is_some() {
            return None;
        }
        let selection = self.selection?;
        let target = match selection.resize_handle_at(point) {
            Some(handle) => DragTarget::Resize(handle),
            None if selection.contains(point) => DragTarget::Move,
            None => return None,
        };
        self.drag = Some(Drag {
            target,
            start: point,
            baseline: self.edit_state(),
        });
        Some(target)
    }

    /// Updates the live drag preview; nothing enters history until release.
    pub fn update_drag(&mut self, point: [f64; 2]) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        if !point.iter().all(|value| value.is_finite()) {
            return;
        }
        let Some(baseline) = drag.baseline.selection else {
            return;
        };
        let [_, _, screen_width, screen_height] = self.frame.geometry().source_rect();
        let (candidate, annotations) = match drag.target {
            DragTarget::Move => {
                // Total movement from the press, clamped so the selection stays on
                // the single built-in screen. Annotations follow the applied delta,
                // never the raw pointer delta, so relative positions survive clamping.
                // A sub-epsilon overshoot admitted by `pixel_rect` can make the
                // theoretical upper bound negative; widen it instead of panicking
                // in `clamp` when the minimum exceeds the maximum.
                let minimum_dx = -baseline.x();
                let maximum_dx = (screen_width - (baseline.x() + baseline.width())).max(minimum_dx);
                let minimum_dy = -baseline.y();
                let maximum_dy =
                    (screen_height - (baseline.y() + baseline.height())).max(minimum_dy);
                let dx = (point[0] - drag.start[0]).clamp(minimum_dx, maximum_dx);
                let dy = (point[1] - drag.start[1]).clamp(minimum_dy, maximum_dy);
                let Ok(moved) = Selection::try_new(
                    baseline.x() + dx,
                    baseline.y() + dy,
                    baseline.width(),
                    baseline.height(),
                ) else {
                    return;
                };
                let annotations = drag
                    .baseline
                    .annotations
                    .iter()
                    .filter_map(|annotation| annotation.translated(dx, dy))
                    .collect();
                (moved, annotations)
            }
            DragTarget::Resize(handle) => {
                // Each control point moves only its own edges; the opposite edges
                // keep the baseline. A minimum extent prevents inversion or zero.
                let minimum_width = baseline.width().min(MIN_SELECTION_POINTS);
                let minimum_height = baseline.height().min(MIN_SELECTION_POINTS);
                let mut left = baseline.x();
                let mut right = baseline.x() + baseline.width();
                let mut top = baseline.y();
                let mut bottom = baseline.y() + baseline.height();
                match handle {
                    // Screen bounds and the minimum extent can meet at a rounding
                    // slop; widen the range so `clamp` never sees min > max.
                    ResizeHandle::TopLeft => {
                        left = point[0].clamp(0.0, (right - minimum_width).max(0.0));
                        top = point[1].clamp(0.0, (bottom - minimum_height).max(0.0));
                    }
                    ResizeHandle::Top => {
                        top = point[1].clamp(0.0, (bottom - minimum_height).max(0.0));
                    }
                    ResizeHandle::TopRight => {
                        let upper = screen_width.max(left + minimum_width);
                        right = point[0].clamp(left + minimum_width, upper);
                        top = point[1].clamp(0.0, (bottom - minimum_height).max(0.0));
                    }
                    ResizeHandle::Right => {
                        let upper = screen_width.max(left + minimum_width);
                        right = point[0].clamp(left + minimum_width, upper);
                    }
                    ResizeHandle::BottomRight => {
                        let upper_x = screen_width.max(left + minimum_width);
                        let upper_y = screen_height.max(top + minimum_height);
                        right = point[0].clamp(left + minimum_width, upper_x);
                        bottom = point[1].clamp(top + minimum_height, upper_y);
                    }
                    ResizeHandle::Bottom => {
                        let upper = screen_height.max(top + minimum_height);
                        bottom = point[1].clamp(top + minimum_height, upper);
                    }
                    ResizeHandle::BottomLeft => {
                        left = point[0].clamp(0.0, (right - minimum_width).max(0.0));
                        let upper = screen_height.max(top + minimum_height);
                        bottom = point[1].clamp(top + minimum_height, upper);
                    }
                    ResizeHandle::Left => {
                        left = point[0].clamp(0.0, (right - minimum_width).max(0.0));
                    }
                }
                let Ok(resized) = Selection::try_new(left, top, right - left, bottom - top) else {
                    return;
                };
                (resized, drag.baseline.annotations.clone())
            }
        };
        // The single document limit check stays ahead of every applied resize/move.
        if candidate.pixel_rect(self.frame.geometry()).is_err() {
            return;
        }
        self.selection = Some(candidate);
        self.annotations = annotations;
    }

    /// Ends the active drag, recording one history step only when state changed.
    ///
    /// # Errors
    /// Returns [`DocumentError::VersionExhausted`] when the history version cannot
    /// advance; the pre-drag state is restored first.
    pub fn end_drag(&mut self) -> Result<bool, DocumentError> {
        let Some(drag) = self.drag.take() else {
            return Ok(false);
        };
        if self.edit_state() == drag.baseline {
            return Ok(false);
        }
        let version = match self.next_version() {
            Ok(version) => version,
            Err(error) => {
                self.restore(drag.baseline);
                return Err(error);
            }
        };
        self.commit(drag.baseline, version);
        Ok(true)
    }

    /// Whether a drag is currently held, so overlay keys can ignore it.
    #[must_use]
    pub const fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Undoes the most recent committed edit.
    ///
    /// # Errors
    /// Returns [`DocumentError::VersionExhausted`] when the version cannot advance.
    pub fn undo(&mut self) -> Result<bool, DocumentError> {
        if self.drag.is_some() {
            return Ok(false);
        }
        let Some(previous) = self.history.undo.pop() else {
            return Ok(false);
        };
        let version = match self.next_version() {
            Ok(version) => version,
            Err(error) => {
                self.history.undo.push(previous);
                return Err(error);
            }
        };
        let current = self.edit_state();
        self.restore(previous);
        self.history.redo.push(current);
        self.version = version;
        Ok(true)
    }

    /// Reapplies the most recently undone edit.
    ///
    /// # Errors
    /// Returns [`DocumentError::VersionExhausted`] when the version cannot advance.
    pub fn redo(&mut self) -> Result<bool, DocumentError> {
        if self.drag.is_some() {
            return Ok(false);
        }
        let Some(next) = self.history.redo.pop() else {
            return Ok(false);
        };
        let version = match self.next_version() {
            Ok(version) => version,
            Err(error) => {
                self.history.redo.push(next);
                return Err(error);
            }
        };
        let current = self.edit_state();
        self.restore(next);
        self.history.undo.push(current);
        self.version = version;
        Ok(true)
    }

    fn edit_state(&self) -> EditState {
        EditState {
            selection: self.selection,
            annotations: self.annotations.clone(),
        }
    }

    fn restore(&mut self, state: EditState) {
        self.selection = state.selection;
        self.annotations = state.annotations;
    }

    fn next_version(&self) -> Result<u64, DocumentError> {
        self.version
            .checked_add(1)
            .ok_or(DocumentError::VersionExhausted)
    }

    fn commit(&mut self, previous: EditState, version: u64) {
        self.history.undo.push(previous);
        self.history.redo.clear();
        self.version = version;
    }

    /// Replaces the selection after validating its physical output rectangle.
    ///
    /// A reselect is one undoable step that also replaces every annotation,
    /// without prompting. Selecting the same rectangle with no annotations is not
    /// a change and does not bump the version. A rejected rectangle leaves the
    /// existing document untouched.
    ///
    /// # Errors
    /// Returns the [`Selection::pixel_rect`] error when the region is outside the
    /// captured screen, empty after 2x rounding, or over the output limits, and
    /// [`DocumentError::VersionExhausted`] when the history version cannot advance.
    pub fn select(&mut self, selection: Selection) -> Result<(), DocumentError> {
        selection.pixel_rect(self.frame.geometry())?;
        if self.drag.is_some() {
            return Ok(());
        }
        if self.selection == Some(selection) && self.annotations.is_empty() {
            return Ok(());
        }
        let version = self.next_version()?;
        let previous = self.edit_state();
        self.selection = Some(selection);
        self.annotations.clear();
        self.commit(previous, version);
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
