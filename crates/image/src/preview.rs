//! Bounded presentation-only sampling; the immutable physical-pixel base is retained.
use arcshot_core::CapturedFrame;
use std::{error::Error, fmt};

/// V1's physical-pixel cap for a preview surface, independent of the document cap.
const MAX_PREVIEW_PIXELS: u32 = 4_000_000;

/// Packed opaque BGRA pixels intended only for presenting the frozen screen.
#[derive(Debug)]
pub struct CapturePreview {
    /// Preview width in physical pixels.
    pub width: u32,
    /// Preview height in physical pixels.
    pub height: u32,
    /// Packed BGRA bytes, already in named sRGB and fully opaque.
    pub bgra: Vec<u8>,
}

/// Creates a bounded preview without changing or downscaling the authoritative base.
///
/// Integer nearest-neighbor sampling is only a UI preview, never output data. Source
/// pixels come from the previously checked frame contract and do not include padding.
///
/// # Errors
/// Returns an allocation error before any over-budget preview surface is constructed.
pub fn capture_preview(frame: &CapturedFrame) -> Result<CapturePreview, PreviewError> {
    let [width, height] = frame.geometry().pixel_size();
    let mut divisor = 1;
    while width.div_ceil(divisor) * height.div_ceil(divisor) > MAX_PREVIEW_PIXELS {
        divisor += 1;
    }
    let w = width.div_ceil(divisor);
    let h = height.div_ceil(divisor);
    let length = w as usize * h as usize * 4;
    let mut bgra = Vec::new();
    bgra.try_reserve_exact(length).map_err(|_| PreviewError)?;
    for y in 0..h {
        for x in 0..w {
            let offset = (y * divisor) as usize * frame.stride() + (x * divisor) as usize * 4;
            bgra.extend_from_slice(&frame.bgra()[offset..offset + 4]);
        }
    }
    Ok(CapturePreview {
        width: w,
        height: h,
        bgra,
    })
}
/// The preview allocation could not be reserved within the admitted pixel budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewError;
impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not allocate capture preview")
    }
}
impl Error for PreviewError {}
