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
        })
    }

    /// Verifies one new frame against history and extends it only when strictly
    /// unique, fully overlapping and contiguous.
    ///
    /// # Errors
    /// Returns the classified refusal; the builder is left unchanged.
    pub fn submit(&mut self, frame: &CapturedFrame) -> Result<StitchDecision, StitchRefusal> {
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

        let mut exact: Option<i64> = None;
        let mut ambiguous = false;

        // s is the document row of the frame's top edge. Every candidate with
        // at least one overlapping row is shortlisted by row-hash equality,
        // then verified byte-for-byte; disjoint jumps are gaps.
        for s in (self.top - i64::from(viewport) + 1)..=(self.bottom - 1) {
            let overlap_start = self.top.max(s);
            let overlap_end = self.bottom.min(s + i64::from(viewport));
            let overlap_rows = (overlap_end - overlap_start) as usize;
            if overlap_rows == 0 {
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
            for candidate in (self.top - i64::from(viewport) + 1)..=(self.bottom - 1) {
                let overlap_start = self.top.max(candidate);
                let overlap_end = self.bottom.min(candidate + i64::from(viewport));
                let overlap_rows = (overlap_end - overlap_start) as usize;
                if overlap_rows == 0 {
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
                }
            }
            return Err(
                if partial_matches > 0 && partial_matches < partial_overlap {
                    StitchRefusal::ContentChanged
                } else {
                    StitchRefusal::Gap
                },
            );
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
        let mut surface = vec![
            0_u8;
            self.row_bytes
                .checked_mul(new_height as usize)
                .ok_or(StitchRefusal::ResourceLimit)?
        ];
        let mut hashes = vec![0_u64; new_height as usize];
        let old_offset = (self.top - new_top) as usize;
        surface[old_offset * self.row_bytes..old_offset * self.row_bytes + self.surface.len()]
            .copy_from_slice(&self.surface);
        hashes[old_offset..old_offset + self.hashes.len()].copy_from_slice(&self.hashes);
        let frame_offset = (s - new_top) as usize;
        for row in 0..viewport as usize {
            let start = row * frame.stride();
            let target = (frame_offset + row) * self.row_bytes;
            surface[target..target + self.row_bytes]
                .copy_from_slice(&frame.bgra()[start..start + self.row_bytes]);
            hashes[frame_offset + row] = frame_hashes[row];
        }
        let added = new_height - old_height;
        self.surface = surface;
        self.hashes = hashes;
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
        CapturedFrame::try_from_srgb_bgra(
            geometry,
            self.width,
            height,
            self.row_bytes,
            self.surface.clone(),
        )
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
