//! Strict vertical stitching for fixed-viewport Scroll frames (I-11 §4).
//!
//! Every captured frame shows the same physical viewport at a fixed selection
//! and scale. This module verifies one new frame against the already-verified
//! history before it may extend the document: exactly one vertical displacement
//! must explain every overlapping row, the extension must be contiguous new
//! content, and a viewport that lies wholly inside history is a de-duplicated
//! reverse pass. It never guess-fills, warps, blurs a seam or hides a gap, and
//! it performs checked size arithmetic before allocating a larger surface.
use arcshot_core::{CapturedFrame, FrameError, MAX_EDGE, MAX_PIXELS, ScreenGeometry};
use std::{error::Error, fmt};

/// V1 cap on the live verified preview, in physical pixels (I-13).
pub const MAX_PREVIEW_PIXELS: usize = 4_000_000;

/// Why one frame could not extend or de-duplicate against verified history.
///
/// Every variant is a known-state refusal: collection stops without output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StitchRefusal {
    /// The frame geometry differs from the fixed selection or scale.
    Geometry,
    /// More than one vertical displacement explains the overlap exactly.
    Ambiguous,
    /// Some overlapping rows match but the overlap is not fully consistent.
    ContentChanged,
    /// No overlapping row matches: the viewport jumped a gap.
    Gap,
    /// The extension would exceed V1's document pixel or edge limit.
    ResourceLimit,
}

impl fmt::Display for StitchRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Geometry => "scroll frame geometry changed",
            Self::Ambiguous => "scroll displacement is ambiguous",
            Self::ContentChanged => "scroll content changed inside the overlap",
            Self::Gap => "scroll displacement leaves an unverifiable gap",
            Self::ResourceLimit => "stitched scroll document exceeds its budget",
        })
    }
}
impl Error for StitchRefusal {}

/// The document row of a frame's top edge, relative to the origin frame top.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StitchPlacement {
    /// Origin-relative physical row of the frame's first row.
    pub offset: i64,
}

/// Where one frame diverged from verified history inside the best overlap.
///
/// Rows are document rows of the verified history and columns are physical
/// pixel columns, so a native session can separate an edge artifact from
/// genuinely changed content. Recorded only for diagnostics; the refusal
/// itself stays [`StitchRefusal::ContentChanged`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MismatchSpan {
    /// First document row whose bytes differ.
    pub first_row: i64,
    /// Last document row whose bytes differ.
    pub last_row: i64,
    /// First physical column whose bytes differ in any differing row.
    pub first_col: u32,
    /// Last physical column whose bytes differ in any differing row.
    pub last_col: u32,
}

/// One verified decision about a new frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StitchDecision {
    /// New contiguous rows were appended above or below the verified history.
    Extended {
        /// Where the frame sits in verified history.
        placement: StitchPlacement,
        /// Number of newly verified physical rows.
        added_rows: u32,
    },
    /// The whole viewport already lies inside verified history.
    Duplicate {
        /// Where the viewport sits in verified history.
        placement: StitchPlacement,
    },
}

/// A packed opaque BGRA thumbnail of the verified history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StitchPreview {
    /// Thumbnail width in physical pixels.
    pub width: u32,
    /// Thumbnail height in physical pixels.
    pub height: u32,
    /// Packed opaque BGRA pixels in named sRGB.
    pub bgra: Vec<u8>,
}

/// The verified vertical history of one fixed-viewport Scroll collection.
///
/// The surface always contains every verified physical row exactly once. Row
/// order is content order: scrolling down grows `bottom`, scrolling back up
/// grows `top`, and a viewport inside `[top, bottom)` is de-duplicated.
#[derive(Debug)]
pub struct StitchBuilder {
    origin_geometry: ScreenGeometry,
    display_id: u32,
    scale: f64,
    width: u32,
    viewport_height: u32,
    row_bytes: usize,
    /// Packed opaque BGRA rows covering document rows `[top, bottom)`.
    surface: Vec<u8>,
    /// One row hash per surface row, aligned with `surface`.
    hashes: Vec<u64>,
    top: i64,
    bottom: i64,
    extensions: u64,
    duplicates: u64,
    added_rows: u64,
    last_mismatch: Option<MismatchSpan>,
}

impl StitchBuilder {
    /// Starts verified history at the trusted origin frame.
    ///
    /// # Errors
    /// Returns [`StitchRefusal::Geometry`] for a malformed viewport size and
    /// [`StitchRefusal::ResourceLimit`] when the packed surface cannot be built.
    pub fn new(origin: &CapturedFrame) -> Result<Self, StitchRefusal> {
        let geometry = origin.geometry();
        let scale = geometry.scale();
        let [width, height] = geometry.pixel_size();
        if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
            return Err(StitchRefusal::Geometry);
        }
        let row_bytes = (width as usize)
            .checked_mul(4)
            .ok_or(StitchRefusal::ResourceLimit)?;
        let mut surface = Vec::new();
        surface
            .try_reserve_exact(
                row_bytes
                    .checked_mul(height as usize)
                    .ok_or(StitchRefusal::ResourceLimit)?,
            )
            .map_err(|_| StitchRefusal::ResourceLimit)?;
        for row in 0..height as usize {
            let start = row * origin.stride();
            surface.extend_from_slice(&origin.bgra()[start..start + row_bytes]);
        }
        let hashes = surface.chunks_exact(row_bytes).map(hash_row).collect();
        Ok(Self {
            origin_geometry: geometry,
            display_id: geometry.display_id(),
            scale,
            width,
            viewport_height: height,
            row_bytes,
            surface,
            hashes,
            top: 0,
            bottom: i64::from(height),
            extensions: 0,
            duplicates: 0,
            added_rows: 0,
            last_mismatch: None,
        })
    }

    /// Verifies one new frame against history and extends it only when strictly
    /// unique, fully overlapping and contiguous.
    ///
    /// # Errors
    /// Returns the classified refusal; the builder is left unchanged.
    pub fn submit(&mut self, frame: &CapturedFrame) -> Result<StitchDecision, StitchRefusal> {
        self.last_mismatch = None;
        if frame.geometry() != self.origin_geometry {
            return Err(StitchRefusal::Geometry);
        }
        let viewport = self.viewport_height;
        let frame_hashes: Vec<u64> = frame
            .bgra()
            .chunks_exact(frame.stride())
            .map(hash_row)
            .collect();
        if frame_hashes.len() != viewport as usize {
            return Err(StitchRefusal::Geometry);
        }

        // A displacement is only reliable when the overlap is a substantial
        // part of the viewport. A one- or two-row overlap of uniform content
        // (a blank margin at a frame edge) matches many displacements and must
        // not make a uniquely verified large overlap look ambiguous.
        let min_overlap = (viewport / 4).max(1) as usize;

        let mut exact: Option<i64> = None;
        let mut ambiguous = false;

        // s is the document row of the frame's top edge. Every candidate with
        // a reliable overlap is shortlisted by row-hash equality, then
        // verified byte-for-byte; disjoint jumps are gaps.
        for s in (self.top - i64::from(viewport) + 1)..=(self.bottom - 1) {
            let overlap_start = self.top.max(s);
            let overlap_end = self.bottom.min(s + i64::from(viewport));
            let overlap_rows = (overlap_end - overlap_start) as usize;
            if overlap_rows < min_overlap {
                continue;
            }
            let frame_start = (overlap_start - s) as usize;
            let surface_start = (overlap_start - self.top) as usize;
            if frame_hashes[frame_start..frame_start + overlap_rows]
                != self.hashes[surface_start..surface_start + overlap_rows]
            {
                continue;
            }
            if !exact_rows_match(
                frame,
                &self.surface,
                self.row_bytes,
                s,
                self.top,
                overlap_start,
                overlap_end,
            ) {
                continue;
            }
            if exact.is_some() {
                ambiguous = true;
            } else {
                exact = Some(s);
            }
        }

        if ambiguous {
            return Err(StitchRefusal::Ambiguous);
        }
        let Some(s) = exact else {
            // No displacement explains the whole overlap. Count matching rows
            // once more to separate a changed overlap from a disjoint jump.
            let mut partial_matches = 0_usize;
            let mut partial_overlap = 0_usize;
            let mut best_s: Option<i64> = None;
            for candidate in (self.top - i64::from(viewport) + 1)..=(self.bottom - 1) {
                let overlap_start = self.top.max(candidate);
                let overlap_end = self.bottom.min(candidate + i64::from(viewport));
                let overlap_rows = (overlap_end - overlap_start) as usize;
                if overlap_rows < min_overlap {
                    continue;
                }
                let frame_start = (overlap_start - candidate) as usize;
                let surface_start = (overlap_start - self.top) as usize;
                let matches = frame_hashes[frame_start..frame_start + overlap_rows]
                    .iter()
                    .zip(&self.hashes[surface_start..surface_start + overlap_rows])
                    .filter(|(a, b)| a == b)
                    .count();
                if matches > partial_matches {
                    partial_matches = matches;
                    partial_overlap = overlap_rows;
                    best_s = Some(candidate);
                }
            }
            if partial_matches > 0 && partial_matches < partial_overlap {
                if let Some(best) = best_s {
                    self.last_mismatch = mismatch_span(
                        frame,
                        &self.surface,
                        self.row_bytes,
                        best,
                        self.top,
                        self.bottom,
                        viewport,
                    );
                }
                return Err(StitchRefusal::ContentChanged);
            }
            return Err(StitchRefusal::Gap);
        };
        if s >= self.top && s + i64::from(viewport) <= self.bottom {
            self.duplicates += 1;
            return Ok(StitchDecision::Duplicate {
                placement: StitchPlacement { offset: s },
            });
        }

        let new_top = self.top.min(s);
        let new_bottom = self.bottom.max(s + i64::from(viewport));
        let new_height =
            u32::try_from(new_bottom - new_top).map_err(|_| StitchRefusal::ResourceLimit)?;
        self.check_extension(new_height)?;
        let old_height =
            u32::try_from(self.bottom - self.top).map_err(|_| StitchRefusal::ResourceLimit)?;
        // Grow the verified surface in place instead of rebuilding a fresh
        // buffer for every extension: a new allocation per frame both copies
        // the whole document each time and makes the allocator hold every
        // intermediate size, which is what pushed a 24 M document to several
        // GiB of physical footprint. Reserving geometrically bounds both the
        // copies and the live surfaces to the document itself.
        self.extend_surface(new_height, new_top, s, viewport, frame, &frame_hashes)?;
        let added = new_height - old_height;
        self.top = new_top;
        self.bottom = new_bottom;
        self.extensions += 1;
        self.added_rows += u64::from(added);
        Ok(StitchDecision::Extended {
            placement: StitchPlacement { offset: s },
            added_rows: added,
        })
    }

    /// Returns the verified document as one packed opaque BGRA frame.
    ///
    /// The copy is sized to the surface capacity the builder already grows
    /// geometrically, not to the exact document height. A caller that keeps the
    /// latest verified frame (the session does) then re-uses a handful of
    /// allocation sizes instead of allocating a slightly larger buffer for
    /// every single frame, which is what bloated the observed footprint.
    ///
    /// # Errors
    /// Returns [`StitchRefusal::ResourceLimit`] when the checked construction fails.
    pub fn current_frame(&self) -> Result<CapturedFrame, StitchRefusal> {
        let height =
            u32::try_from(self.bottom - self.top).map_err(|_| StitchRefusal::ResourceLimit)?;
        let geometry = ScreenGeometry::try_new(
            self.display_id,
            0.0,
            0.0,
            f64::from(self.width) / self.scale,
            f64::from(height) / self.scale,
            self.scale,
        )
        .map_err(|_| StitchRefusal::ResourceLimit)?;
        let mut bytes = Vec::new();
        let capacity = self.surface.capacity().max(self.surface.len());
        bytes
            .try_reserve_exact(capacity)
            .map_err(|_| StitchRefusal::ResourceLimit)?;
        bytes.extend_from_slice(&self.surface);
        CapturedFrame::try_from_srgb_bgra(geometry, self.width, height, self.row_bytes, bytes)
            .map_err(|error| match error {
                FrameError::Budget
                | FrameError::Dimensions
                | FrameError::Stride
                | FrameError::ByteLength => StitchRefusal::ResourceLimit,
                FrameError::Geometry | FrameError::Alpha => StitchRefusal::Geometry,
            })
    }

    /// Builds a deterministic nearest-neighbour thumbnail within `max_pixels`.
    ///
    /// Returns `None` when `max_pixels` cannot fit even one pixel.
    #[must_use]
    pub fn preview(&self, max_pixels: usize) -> Option<StitchPreview> {
        let height = u32::try_from(self.bottom - self.top).ok()?;
        let area = (self.width as usize).checked_mul(height as usize)?;
        let (out_width, out_height, step) = if area <= max_pixels {
            (self.width, height, 1_u32)
        } else {
            let mut step = ((area as f64 / max_pixels as f64).sqrt().ceil() as u32).max(2);
            loop {
                let out_width = self.width.div_ceil(step);
                let out_height = height.div_ceil(step);
                if out_width
                    .checked_mul(out_height)
                    .is_some_and(|pixels| pixels <= max_pixels as u32)
                {
                    break (out_width, out_height, step);
                }
                step += 1;
            }
        };
        let mut bgra = Vec::new();
        bgra.try_reserve_exact(out_width as usize * out_height as usize * 4)
            .ok()?;
        for row in 0..out_height {
            let source_row = (u64::from(row) * u64::from(step)) as usize * self.row_bytes;
            for column in 0..out_width {
                let start = source_row + (u64::from(column) * u64::from(step)) as usize * 4;
                bgra.extend_from_slice(&self.surface[start..start + 4]);
            }
        }
        Some(StitchPreview {
            width: out_width,
            height: out_height,
            bgra,
        })
    }

    /// Span of the first divergence in the best overlap of the last submit.
    ///
    /// `Some` documented-history row and physical-column span that differs;
    /// `None` unless the most recent submit refused with
    /// [`StitchRefusal::ContentChanged`].
    #[must_use]
    pub const fn last_mismatch_span(&self) -> Option<MismatchSpan> {
        self.last_mismatch
    }

    /// Current verified physical width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Current verified physical height.
    #[must_use]
    pub fn height(&self) -> u32 {
        u32::try_from(self.bottom - self.top).unwrap_or_default()
    }
    /// Verified document rows above the origin frame top.
    #[must_use]
    pub fn top(&self) -> i64 {
        self.top
    }
    /// Verified document rows below the origin frame top.
    #[must_use]
    pub fn bottom(&self) -> i64 {
        self.bottom
    }
    /// Number of successful extensions.
    #[must_use]
    pub const fn extensions(&self) -> u64 {
        self.extensions
    }
    /// Number of de-duplicated reverse passes.
    #[must_use]
    pub const fn duplicates(&self) -> u64 {
        self.duplicates
    }
    /// Total newly verified rows across all extensions.
    #[must_use]
    pub const fn added_rows(&self) -> u64 {
        self.added_rows
    }

    /// Appends or prepends the new rows of one verified extension in place.
    ///
    /// The surface keeps every row in content order; an upward extension shifts
    /// the verified rows to their new offset and writes the frame above them.
    /// Capacity grows geometrically up to the document budget, so repeated
    /// extensions never allocate a fresh full-size buffer.
    fn extend_surface(
        &mut self,
        new_height: u32,
        new_top: i64,
        placement: i64,
        viewport: u32,
        frame: &CapturedFrame,
        frame_hashes: &[u64],
    ) -> Result<(), StitchRefusal> {
        let old_rows =
            usize::try_from(self.bottom - self.top).map_err(|_| StitchRefusal::ResourceLimit)?;
        let new_rows = new_height as usize;
        let new_len = self
            .row_bytes
            .checked_mul(new_rows)
            .ok_or(StitchRefusal::ResourceLimit)?;
        self.reserve_rows(new_height)?;
        self.surface.resize(new_len, 0);
        self.hashes.resize(new_rows, 0);
        let shift_rows =
            usize::try_from(self.top - new_top).map_err(|_| StitchRefusal::ResourceLimit)?;
        if shift_rows > 0 {
            self.surface
                .copy_within(0..old_rows * self.row_bytes, shift_rows * self.row_bytes);
            self.hashes.copy_within(0..old_rows, shift_rows);
        }
        let frame_offset =
            usize::try_from(placement - new_top).map_err(|_| StitchRefusal::ResourceLimit)?;
        for (row, &hash) in frame_hashes.iter().enumerate().take(viewport as usize) {
            let start = row * frame.stride();
            let target = (frame_offset + row) * self.row_bytes;
            self.surface[target..target + self.row_bytes]
                .copy_from_slice(&frame.bgra()[start..start + self.row_bytes]);
            self.hashes[frame_offset + row] = hash;
        }
        Ok(())
    }

    /// Reserves room for `rows` verified rows, doubling up to the budget.
    fn reserve_rows(&mut self, rows: u32) -> Result<(), StitchRefusal> {
        let needed = self
            .row_bytes
            .checked_mul(rows as usize)
            .ok_or(StitchRefusal::ResourceLimit)?;
        if self.surface.capacity() >= needed {
            return Ok(());
        }
        let limit = document_surface_limit(self.width, self.row_bytes);
        let target = needed.saturating_mul(2).min(limit.max(needed)).max(needed);
        let additional = target - self.surface.len();
        let needed_additional = needed - self.surface.len();
        if self.surface.try_reserve_exact(additional).is_err()
            && self.surface.try_reserve_exact(needed_additional).is_err()
        {
            return Err(StitchRefusal::ResourceLimit);
        }
        Ok(())
    }

    fn check_extension(&self, new_height: u32) -> Result<(), StitchRefusal> {
        if new_height > MAX_EDGE {
            return Err(StitchRefusal::ResourceLimit);
        }
        (self.width as usize)
            .checked_mul(new_height as usize)
            .is_some_and(|pixels| pixels <= MAX_PIXELS)
            .then_some(())
            .ok_or(StitchRefusal::ResourceLimit)
    }
}

/// Largest packed surface one verified document may hold, in bytes.
fn document_surface_limit(width: u32, row_bytes: usize) -> usize {
    let rows_by_edge = MAX_EDGE as usize;
    let rows_by_pixels = MAX_PIXELS / (width as usize).max(1);
    rows_by_edge.min(rows_by_pixels).saturating_mul(row_bytes)
}

fn exact_rows_match(
    frame: &CapturedFrame,
    surface: &[u8],
    row_bytes: usize,
    s: i64,
    top: i64,
    overlap_start: i64,
    overlap_end: i64,
) -> bool {
    for document_row in overlap_start..overlap_end {
        let frame_row = (document_row - s) as usize;
        let surface_row = (document_row - top) as usize;
        let frame_start = frame_row * frame.stride();
        let surface_start = surface_row * row_bytes;
        if frame.bgra()[frame_start..frame_start + row_bytes]
            != surface[surface_start..surface_start + row_bytes]
        {
            return false;
        }
    }
    true
}

/// The document-row and physical-column span where one frame's bytes differ
/// from the verified surface over the overlap at displacement `s`.
///
/// Returns `None` when the row hashes disagreed but the bytes did not (a
/// 64-bit hash collision); no bogus span is reported in that case.
fn mismatch_span(
    frame: &CapturedFrame,
    surface: &[u8],
    row_bytes: usize,
    s: i64,
    top: i64,
    bottom: i64,
    viewport: u32,
) -> Option<MismatchSpan> {
    let overlap_start = top.max(s);
    let overlap_end = bottom.min(s + i64::from(viewport));
    let mut result: Option<MismatchSpan> = None;
    for document_row in overlap_start..overlap_end {
        let frame_row = (document_row - s) as usize;
        let surface_row = (document_row - top) as usize;
        let frame_start = frame_row * frame.stride();
        let surface_start = surface_row * row_bytes;
        let frame_bytes = &frame.bgra()[frame_start..frame_start + row_bytes];
        let surface_bytes = &surface[surface_start..surface_start + row_bytes];
        if frame_bytes == surface_bytes {
            continue;
        }
        let mut span = MismatchSpan {
            first_row: document_row,
            last_row: document_row,
            first_col: u32::MAX,
            last_col: 0,
        };
        for (index, (a, b)) in frame_bytes.iter().zip(surface_bytes).enumerate() {
            if a != b {
                let column = (index / 4) as u32;
                span.first_col = span.first_col.min(column);
                span.last_col = span.last_col.max(column);
            }
        }
        result = Some(match result {
            None => span,
            Some(mut accumulated) => {
                accumulated.last_row = span.last_row;
                accumulated.first_col = accumulated.first_col.min(span.first_col);
                accumulated.last_col = accumulated.last_col.max(span.last_col);
                accumulated
            }
        });
    }
    result
}

/// A deterministic 64-bit FNV-1a style row hash used only to shortlist
/// candidates before exact byte comparison.
fn hash_row(row: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for chunk in row.as_chunks::<8>().0 {
        let value = u64::from_le_bytes(*chunk);
        hash ^= value;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let remainder = row.len() % 8;
    for byte in &row[row.len() - remainder..] {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcshot_core::{CapturedFrame, ScreenGeometry};

    fn make_frame(rows: &[u32]) -> CapturedFrame {
        let height = rows.len() as u32;
        const WIDTH: u32 = 4;
        let mut bgra = Vec::new();
        for &row in rows {
            for _ in 0..WIDTH {
                bgra.extend_from_slice(&[row as u8, (row >> 8) as u8, 7, 255]);
            }
        }
        let geometry = ScreenGeometry::try_new(
            1,
            0.0,
            0.0,
            f64::from(WIDTH) / 2.0,
            f64::from(height) / 2.0,
            2.0,
        )
        .expect("test geometry");
        CapturedFrame::try_from_srgb_bgra(geometry, WIDTH, height, WIDTH as usize * 4, bgra)
            .expect("test frame")
    }

    fn document_rows(frame: &CapturedFrame) -> Vec<u32> {
        let stride = frame.stride();
        frame
            .bgra()
            .chunks_exact(stride)
            .map(|row| u32::from_le_bytes([row[0], row[1], 0, 0]))
            .collect()
    }

    #[test]
    fn downward_extension_is_unique_and_contiguous() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let decision = builder.submit(&make_frame(&[2, 3, 4, 5])).expect("extend");
        assert_eq!(
            decision,
            StitchDecision::Extended {
                placement: StitchPlacement { offset: 2 },
                added_rows: 2
            }
        );
        let frame = builder.current_frame().expect("frame");
        assert_eq!(document_rows(&frame), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(builder.top(), 0);
        assert_eq!(builder.bottom(), 6);
    }

    #[test]
    fn upward_extension_is_unique_and_contiguous() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let decision = builder
            .submit(&make_frame(&[12, 13, 0, 1]))
            .expect("extend above");
        assert_eq!(
            decision,
            StitchDecision::Extended {
                placement: StitchPlacement { offset: -2 },
                added_rows: 2
            }
        );
        let frame = builder.current_frame().expect("frame");
        assert_eq!(document_rows(&frame), vec![12, 13, 0, 1, 2, 3]);
        assert_eq!(builder.top(), -2);
        assert_eq!(builder.bottom(), 4);
    }

    #[test]
    fn reverse_pass_inside_history_is_deduplicated_not_reextended() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        builder.submit(&make_frame(&[2, 3, 4, 5])).expect("down");
        let before = builder.current_frame().expect("before").bgra().to_vec();
        let decision = builder.submit(&make_frame(&[1, 2, 3, 4])).expect("reverse");
        assert_eq!(
            decision,
            StitchDecision::Duplicate {
                placement: StitchPlacement { offset: 1 }
            }
        );
        assert_eq!(builder.current_frame().expect("after").bgra(), before);
        assert_eq!(builder.extensions(), 1);
        assert_eq!(builder.duplicates(), 1);
        // Continuing forward afterwards still appends exactly once.
        builder.submit(&make_frame(&[4, 5, 6, 7])).expect("forward");
        assert_eq!(
            document_rows(&builder.current_frame().expect("frame")),
            vec![0, 1, 2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn identical_viewport_without_scrolling_is_a_duplicate() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        assert_eq!(
            builder.submit(&make_frame(&[0, 1, 2, 3])).expect("same"),
            StitchDecision::Duplicate {
                placement: StitchPlacement { offset: 0 }
            }
        );
        assert_eq!(builder.height(), 4);
    }

    #[test]
    fn uniform_content_is_ambiguous_and_refuses() {
        let origin = make_frame(&[9, 9, 9, 9]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        assert_eq!(
            builder.submit(&make_frame(&[9, 9, 9, 9])),
            Err(StitchRefusal::Ambiguous)
        );
        assert_eq!(builder.height(), 4);
    }

    #[test]
    fn tiny_uniform_edge_overlap_does_not_make_a_unique_extension_ambiguous() {
        // A real text viewport has blank rows at its edges. After a normal
        // extension, the new frame's blank top row can coincide with the
        // history's blank bottom row; that one-row coincidence used to look
        // like a second displacement and refuse a uniquely verified large
        // overlap as ambiguous.
        let origin = make_frame(&[0, 1, 99, 3, 4, 5, 6, 99]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let decision = builder
            .submit(&make_frame(&[99, 3, 4, 5, 6, 99, 8, 9]))
            .expect("extend");
        assert_eq!(
            decision,
            StitchDecision::Extended {
                placement: StitchPlacement { offset: 2 },
                added_rows: 2
            }
        );
        assert_eq!(builder.height(), 10);
        assert_eq!(builder.extensions(), 1);
    }

    #[test]
    fn changed_overlap_is_content_changed() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        // Claiming displacement 2 requires document rows 2 and 3 to match, but
        // the second overlapping row was replaced.
        assert_eq!(
            builder.submit(&make_frame(&[2, 99, 4, 5])),
            Err(StitchRefusal::ContentChanged)
        );
    }

    #[test]
    fn image_stitch_59_01_content_changed_records_diverging_span() {
        // A capture that disagrees with verified history inside the overlap
        // must say where: the document rows and physical columns that differ
        // are the only diagnostic that can separate an edge artifact from
        // changed content during a native session.
        let origin = make_pixel_frame(&pixel_rows(&[10, 20, 30, 40]));
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let mut next = pixel_rows(&[30, 40, 50, 60]);
        // Corrupt the B channel of column 2 in the overlapping row for
        // document row 3 (frame row 1 at displacement 2).
        next[1][2 * 4] = 0x7f;
        assert_eq!(
            builder.submit(&make_pixel_frame(&next)),
            Err(StitchRefusal::ContentChanged)
        );
        assert_eq!(
            builder.last_mismatch_span(),
            Some(MismatchSpan {
                first_row: 3,
                last_row: 3,
                first_col: 2,
                last_col: 2,
            })
        );
        // A refused submit leaves history untouched.
        assert_eq!(builder.height(), 4);
    }

    #[test]
    fn image_stitch_59_02_successful_submit_clears_mismatch_span() {
        let origin = make_pixel_frame(&pixel_rows(&[10, 20, 30, 40]));
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let mut changed = pixel_rows(&[30, 40, 50, 60]);
        changed[0][0] = 0x7f;
        assert_eq!(
            builder.submit(&make_pixel_frame(&changed)),
            Err(StitchRefusal::ContentChanged)
        );
        assert!(builder.last_mismatch_span().is_some());
        builder
            .submit(&make_pixel_frame(&pixel_rows(&[30, 40, 50, 60])))
            .expect("clean extension");
        assert_eq!(builder.last_mismatch_span(), None);
    }

    #[test]
    fn image_stitch_59_03_inner_preview_top_band_is_refused_as_full_width_change() {
        // #59 regression: under the inner fixed preview, the tall-selection
        // 24M build saw the first ~48 rows of each scrolled frame replaced
        // by the target window's top-edge redraw transient, across the whole
        // width. The verifier must refuse the frame (never relax the overlap
        // check or guess-fill), and its diagnostic must localize the change
        // to that band and every physical column.
        const VIEWPORT: u32 = 128;
        let origin = make_pixel_frame(&content_rows(0, VIEWPORT));
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let mut next = content_rows(32, VIEWPORT);
        for row in next.iter_mut().take(48) {
            row.fill(0xf4);
            for pixel in row.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        assert_eq!(
            builder.submit(&make_pixel_frame(&next)),
            Err(StitchRefusal::ContentChanged)
        );
        assert_eq!(
            builder.last_mismatch_span(),
            Some(MismatchSpan {
                first_row: 32,
                last_row: 79,
                first_col: 0,
                last_col: 3,
            })
        );
        assert_eq!(builder.height(), VIEWPORT);
        assert_eq!(builder.extensions(), 0);
    }

    /// Rows whose bytes encode their content row, so any replaced band differs
    /// in every physical column rather than only in the left margin.
    fn content_rows(offset: u32, height: u32) -> Vec<Vec<u8>> {
        (0..height)
            .map(|row| {
                let value = (offset + row) as u8;
                let mut bytes = Vec::new();
                for column in 0..4_u8 {
                    bytes.extend_from_slice(&[value, value.wrapping_add(column), 7, 255]);
                }
                bytes
            })
            .collect()
    }

    fn pixel_rows(fills: &[u8]) -> Vec<Vec<u8>> {
        fills
            .iter()
            .map(|&fill| {
                let mut row = Vec::new();
                for _ in 0..4 {
                    row.extend_from_slice(&[fill, fill, fill, 255]);
                }
                row
            })
            .collect()
    }

    fn make_pixel_frame(rows: &[Vec<u8>]) -> CapturedFrame {
        let height = rows.len() as u32;
        const WIDTH: u32 = 4;
        let mut bgra = Vec::new();
        for row in rows {
            assert_eq!(row.len(), WIDTH as usize * 4);
            bgra.extend_from_slice(row);
        }
        let geometry = ScreenGeometry::try_new(
            1,
            0.0,
            0.0,
            f64::from(WIDTH) / 2.0,
            f64::from(height) / 2.0,
            2.0,
        )
        .expect("test geometry");
        CapturedFrame::try_from_srgb_bgra(geometry, WIDTH, height, WIDTH as usize * 4, bgra)
            .expect("test frame")
    }

    #[test]
    fn disjoint_jump_is_a_gap() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        assert_eq!(
            builder.submit(&make_frame(&[5, 6, 7, 8])),
            Err(StitchRefusal::Gap)
        );
    }

    #[test]
    fn geometry_change_is_refused() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        assert_eq!(
            builder.submit(&make_frame(&[0, 1, 2])),
            Err(StitchRefusal::Geometry)
        );
    }

    #[test]
    fn extension_past_the_edge_limit_is_refused_before_allocation() {
        // A one-column viewport already at the V1 edge limit; a strictly
        // verified upward extension would exceed the edge and must refuse.
        let origin = make_single_column_frame(&(0..MAX_EDGE).collect::<Vec<_>>());
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        let half = MAX_EDGE / 2;
        let mut above = vec![0_u32; MAX_EDGE as usize];
        // The overlapping lower half repeats document rows 0..half exactly;
        // the upper half is new unique content, so displacement is unique.
        for (index, value) in above.iter_mut().enumerate() {
            *value = if index < half as usize {
                (u32::try_from(index).expect("index") + 100_000) + index as u32
            } else {
                u32::try_from(index - half as usize).expect("index")
            };
        }
        assert_eq!(
            builder.submit(&make_single_column_frame(&above)),
            Err(StitchRefusal::ResourceLimit)
        );
        assert_eq!(builder.height(), MAX_EDGE);
    }

    fn make_single_column_frame(rows: &[u32]) -> CapturedFrame {
        let height = rows.len() as u32;
        let mut bgra = Vec::new();
        for &row in rows {
            bgra.extend_from_slice(&[row as u8, (row >> 8) as u8, (row >> 16) as u8, 255]);
        }
        let geometry = ScreenGeometry::try_new(1, 0.0, 0.0, 0.5, f64::from(height) / 2.0, 2.0)
            .expect("test geometry");
        CapturedFrame::try_from_srgb_bgra(geometry, 1, height, 4, bgra).expect("test frame")
    }

    #[test]
    fn image_limit_38_02_alternating_extensions_keep_content_order_in_place() {
        // The verified surface grows in place now, so upward extensions shift
        // the existing rows instead of rebuilding the buffer. Alternating up and
        // down extensions must still keep every row in content order exactly once.
        let origin = make_frame(&[0, 1, 2, 3]);
        let mut builder = StitchBuilder::new(&origin).expect("origin");
        builder.submit(&make_frame(&[2, 3, 4, 5])).expect("down");
        builder.submit(&make_frame(&[254, 255, 0, 1])).expect("up");
        assert_eq!(
            document_rows(&builder.current_frame().expect("frame")),
            vec![254, 255, 0, 1, 2, 3, 4, 5]
        );
        builder.submit(&make_frame(&[4, 5, 6, 7])).expect("down");
        builder
            .submit(&make_frame(&[252, 253, 254, 255]))
            .expect("up");
        assert_eq!(
            document_rows(&builder.current_frame().expect("frame")),
            vec![252, 253, 254, 255, 0, 1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(builder.top(), -4);
        assert_eq!(builder.bottom(), 8);
        assert_eq!(builder.extensions(), 4);
        assert_eq!(builder.added_rows(), 8);
    }

    #[test]
    fn preview_downscales_deterministically_within_the_pixel_cap() {
        let rows: Vec<u32> = (0..10).collect();
        let origin = make_frame(&rows);
        let builder = StitchBuilder::new(&origin).expect("origin");
        let preview = builder.preview(25).expect("preview");
        assert_eq!((preview.width, preview.height), (2, 5));
        assert_eq!(preview.width as usize * preview.height as usize, 10);
        assert_eq!(preview.bgra.len(), 10 * 4);
        assert_eq!(&preview.bgra[0..4], &[0, 0, 7, 255]);
        // Nearest-neighbour step is 2: the last sample is source row 8,
        // column 2 (the rightmost sampled column).
        let last = (4 * 2 + 1) * 4;
        assert_eq!(&preview.bgra[last..last + 4], &[8, 0, 7, 255]);
    }

    #[test]
    fn preview_keeps_native_size_when_already_within_cap() {
        let origin = make_frame(&[0, 1, 2, 3]);
        let builder = StitchBuilder::new(&origin).expect("origin");
        let preview = builder.preview(MAX_PREVIEW_PIXELS).expect("preview");
        assert_eq!((preview.width, preview.height), (4, 4));
    }
}
