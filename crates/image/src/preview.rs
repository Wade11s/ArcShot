//! Native-resolution frozen presentation; the immutable captured base is retained.
use arcshot_core::CapturedFrame;
use std::{error::Error, fmt};

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

/// Creates a native-resolution frozen surface without modifying the captured base.
///
/// Copies packed rows from the admitted frame, dropping only row padding. Frozen
/// presentation shares the document pixel limit and its full-surface memory budget;
/// the separate thumbnail limit does not downsample the screen background.
///
/// # Errors
/// Returns an allocation error if the admitted full-resolution surface cannot be reserved.
pub fn capture_preview(frame: &CapturedFrame) -> Result<CapturePreview, PreviewError> {
    let [width, height] = frame.geometry().pixel_size();
    let row_bytes = width as usize * 4;
    let length = row_bytes * height as usize;
    let mut bgra = Vec::new();
    bgra.try_reserve_exact(length).map_err(|_| PreviewError)?;
    for row in frame.bgra().chunks_exact(frame.stride()) {
        bgra.extend_from_slice(&row[..row_bytes]);
    }
    Ok(CapturePreview {
        width,
        height,
        bgra,
    })
}
/// The frozen surface allocation could not be reserved within the admitted frame budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreviewError;
impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not allocate capture preview")
    }
}
impl Error for PreviewError {}
