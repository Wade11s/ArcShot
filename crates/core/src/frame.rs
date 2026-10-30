//! Checked screen geometry and neutral, opaque sRGB capture data.
use std::{error::Error, fmt, sync::Arc};

/// V1 document limit in physical pixels, including a full-screen base image.
pub const MAX_PIXELS: usize = 24_000_000;
/// V1 maximum physical length of either image edge.
pub const MAX_EDGE: u32 = 16_384;

/// An immutable built-in screen identity with distinct screen and source coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenGeometry {
    id: u32,
    origin: [f64; 2],
    logical_size: [f64; 2],
    pixels: [u32; 2],
    scale: f64,
}

impl ScreenGeometry {
    /// Validates V1's single built-in, 2x screen geometry before size conversion.
    ///
    /// # Errors
    /// Returns an error for nonfinite, fractional, unsupported or over-budget geometry.
    pub fn try_new(
        id: u32,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        scale: f64,
    ) -> Result<Self, FrameError> {
        if id == 0
            || ![x, y, width, height, scale].iter().all(|v| v.is_finite())
            || width <= 0.0
            || height <= 0.0
            || scale != 2.0
        {
            return Err(FrameError::Geometry);
        }
        let w = width * scale;
        let h = height * scale;
        if w.fract() != 0.0
            || h.fract() != 0.0
            || w > f64::from(MAX_EDGE)
            || h > f64::from(MAX_EDGE)
        {
            return Err(FrameError::Dimensions);
        }
        let pixels = [w as u32, h as u32];
        checked_surface(pixels[0], pixels[1])?;
        Ok(Self {
            id,
            origin: [x, y],
            logical_size: [width, height],
            pixels,
            scale,
        })
    }

    /// Returns the native display identity, never an external fallback.
    #[must_use]
    pub const fn display_id(self) -> u32 {
        self.id
    }
    /// Returns the global screen origin in logical points.
    #[must_use]
    pub const fn screen_origin(self) -> [f64; 2] {
        self.origin
    }
    /// Returns the display-local full-screen capture rectangle in logical points.
    #[must_use]
    pub const fn source_rect(self) -> [f64; 4] {
        [0.0, 0.0, self.logical_size[0], self.logical_size[1]]
    }
    /// Returns the physical pixel width and height.
    #[must_use]
    pub const fn pixel_size(self) -> [u32; 2] {
        self.pixels
    }
    /// Returns the capture filter's point-to-pixel scale.
    #[must_use]
    pub const fn scale(self) -> f64 {
        self.scale
    }
}

fn checked_surface(width: u32, height: u32) -> Result<usize, FrameError> {
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return Err(FrameError::Dimensions);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(FrameError::Dimensions)?;
    if pixels > MAX_PIXELS {
        return Err(FrameError::Budget);
    }
    pixels.checked_mul(4).ok_or(FrameError::Dimensions)
}

/// An immutable, named-sRGB, opaque, 8-bit BGRA frame without native handles.
///
/// Clones share the same byte allocation; history and presentation never copy the base.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedFrame {
    geometry: ScreenGeometry,
    stride: usize,
    bytes: Arc<Vec<u8>>,
}

impl CapturedFrame {
    /// Admits native pixels only after named-sRGB CGContext conversion by the adapter.
    ///
    /// This constructor checks structure and opacity, not color provenance. Callers must
    /// convert using a named sRGB color space before calling it; arbitrary bytes cannot
    /// prove their color space. Padding is retained but never treated as image pixels.
    ///
    /// # Errors
    /// Rejects mismatched geometry, stride, length, overflow, budget or nonopaque pixels.
    pub fn try_from_srgb_bgra(
        geometry: ScreenGeometry,
        width: u32,
        height: u32,
        stride: usize,
        bytes: Vec<u8>,
    ) -> Result<Self, FrameError> {
        let surface = checked_surface(width, height)?;
        if geometry.pixel_size() != [width, height] {
            return Err(FrameError::Dimensions);
        }
        let row = (width as usize).checked_mul(4).ok_or(FrameError::Stride)?;
        let length = stride
            .checked_mul(height as usize)
            .ok_or(FrameError::Stride)?;
        // Reserve room for native/normalized frames, the full-resolution frozen
        // surface, GPUI upload/texture and snapshots (8 full surfaces), plus
        // thumbnail/runtime headroom.
        // This is admission accounting, not a claim of measured physical footprint.
        const CAPTURE_HEADROOM: usize = 272 * 1024 * 1024;
        const PROCESS_BUDGET: usize = 2 * 1024 * 1024 * 1024;
        if stride < row || length < surface {
            return Err(FrameError::Stride);
        }
        if length
            .checked_mul(8)
            .and_then(|v| v.checked_add(CAPTURE_HEADROOM))
            .is_none_or(|v| v > PROCESS_BUDGET)
        {
            return Err(FrameError::Budget);
        }
        if bytes.len() != length {
            return Err(FrameError::ByteLength);
        }
        if bytes.chunks_exact(stride).any(|row| {
            row[..width as usize * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] != 255)
        }) {
            return Err(FrameError::Alpha);
        }
        Ok(Self {
            geometry,
            stride,
            bytes: Arc::new(bytes),
        })
    }

    /// Returns the capture screen snapshot used for later invalidation.
    #[must_use]
    pub const fn geometry(&self) -> ScreenGeometry {
        self.geometry
    }
    /// Returns bytes per row, including native alignment padding.
    #[must_use]
    pub const fn stride(&self) -> usize {
        self.stride
    }
    /// Borrows opaque BGRA bytes; no copy or disk persistence occurs.
    #[must_use]
    pub fn bgra(&self) -> &[u8] {
        &self.bytes
    }
}

/// Capture data rejected before becoming a document or allocating a derived surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// Screen identity, coordinates or 2x scale are invalid.
    Geometry,
    /// Physical dimensions are invalid or exceed the edge limit.
    Dimensions,
    /// The full surface exceeds V1's pixel budget.
    Budget,
    /// Row layout is shorter than its pixels or overflows.
    Stride,
    /// The buffer does not exactly match stride times height.
    ByteLength,
    /// At least one image pixel is not fully opaque.
    Alpha,
}
impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Geometry => "invalid built-in screen geometry",
            Self::Dimensions => "invalid capture dimensions",
            Self::Budget => "capture exceeds pixel budget",
            Self::Stride => "invalid capture stride",
            Self::ByteLength => "invalid capture byte length",
            Self::Alpha => "capture is not opaque",
        })
    }
}
impl Error for FrameError {}
