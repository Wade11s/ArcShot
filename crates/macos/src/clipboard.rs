//! Safe AppKit clipboard and completion-sound adapters.
use objc2::AnyThread;
use objc2_app_kit::{NSBitmapImageRep, NSPasteboard, NSPasteboardTypePNG, NSSound};
use objc2_foundation::{NSData, NSString};
use std::{error::Error, fmt};

/// Writes PNG bytes to the general pasteboard and verifies the declared type.
///
/// Writing is not treated as success: `setData` must return true and the PNG
/// type must be readable back. Nothing is written to disk.
///
/// # Errors
/// Returns [`ClipboardError::WriteRejected`] when AppKit refuses the data and
/// [`ClipboardError::TypeUnavailable`] when PNGf is not exposed afterwards.
pub fn write_png_to_clipboard(png: &[u8]) -> Result<(), ClipboardError> {
    let data = NSData::with_bytes(png);
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: `NSPasteboardTypePNG` is an immutable AppKit constant.
    let png_type = unsafe { NSPasteboardTypePNG };
    if !pasteboard.setData_forType(Some(&data), png_type) {
        return Err(ClipboardError::WriteRejected);
    }
    if pasteboard.dataForType(png_type).is_none() {
        return Err(ClipboardError::TypeUnavailable);
    }
    Ok(())
}

/// Plays one short system completion sound. A failed sound never fails output.
#[must_use]
pub fn play_completion_sound() -> bool {
    NSSound::soundNamed(&NSString::from_str("Tink")).is_some_and(|sound| sound.play())
}

/// Result of a named-pasteboard PNG roundtrip used by the native smoke test.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClipboardSmoke {
    /// Decoded PNG width.
    pub width: u32,
    /// Decoded PNG height.
    pub height: u32,
}

/// Validates the native PNGf path without touching the user's clipboard.
///
/// Writes to a process-private named pasteboard, checks the byte roundtrip and
/// decodes the PNG in memory with AppKit. The private pasteboard is cleared
/// afterwards; no screenshot data is persisted.
///
/// # Errors
/// Returns the write/type errors above or [`ClipboardError::Decode`] when
/// AppKit cannot read the bytes back as a PNG of the expected size.
pub fn validate_png_pasteboard(
    png: &[u8],
    expected_width: u32,
    expected_height: u32,
) -> Result<ClipboardSmoke, ClipboardError> {
    let pasteboard =
        NSPasteboard::pasteboardWithName(&NSString::from_str("dev.arcshot.clipboard-smoke"));
    let data = NSData::with_bytes(png);
    // SAFETY: `NSPasteboardTypePNG` is an immutable AppKit constant.
    let png_type = unsafe { NSPasteboardTypePNG };
    pasteboard.clearContents();
    if !pasteboard.setData_forType(Some(&data), png_type) {
        return Err(ClipboardError::WriteRejected);
    }
    let read_back = pasteboard
        .dataForType(png_type)
        .ok_or(ClipboardError::TypeUnavailable)?;
    if read_back.to_vec() != png {
        return Err(ClipboardError::Decode);
    }
    let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &read_back)
        .ok_or(ClipboardError::Decode)?;
    let width = u32::try_from(bitmap.pixelsWide()).map_err(|_| ClipboardError::Decode)?;
    let height = u32::try_from(bitmap.pixelsHigh()).map_err(|_| ClipboardError::Decode)?;
    pasteboard.clearContents();
    if (width, height) != (expected_width, expected_height) {
        return Err(ClipboardError::Decode);
    }
    Ok(ClipboardSmoke { width, height })
}

/// Native clipboard failures that never count as a successful copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardError {
    /// AppKit rejected the PNG data.
    WriteRejected,
    /// The pasteboard did not expose the PNG type after writing.
    TypeUnavailable,
    /// The PNG could not be decoded or did not match the composed size.
    Decode,
}
impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WriteRejected => "clipboard rejected the PNG data",
            Self::TypeUnavailable => "clipboard did not expose the PNG type",
            Self::Decode => "clipboard PNG did not decode as the composed image",
        })
    }
}
impl Error for ClipboardError {}
