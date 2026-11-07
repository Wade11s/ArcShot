//! Safe explicit-save adapters: the native PNG panel and checked file writes.
//!
//! # Main-thread contract and lifecycle
//!
//! `NSSavePanel` and `NSData` file writes are AppKit/Foundation UI-side work and
//! run only on the main thread; every entry point returns `NotMainThread`
//! instead of trusting a worker. `PngSavePanel::begin` shows the panel modelessly
//! and returns immediately, so no nested modal run loop can re-enter GPUI while
//! the coordinator owns the output mutex. The frozen overlay is a GPUI `PopUp`
//! window at `NSPopUpWindowLevel` (101); the panel is raised above it so it stays
//! visible and key while the overlay remains behind it and rejects editing.
//!
//! AppKit's completion handler stores exactly one outcome in a process slot; the
//! app drains it from a later main-thread tick. The retained panel lives in a
//! main-thread slot until that drain, so AppKit never releases the panel inside
//! its own callback. Nothing is written before the user chooses a destination,
//! and a cancel writes nothing at all.
use arcshot_core::MAX_PNG_BYTES;
use block2::RcBlock;
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSModalResponse, NSModalResponseCancel, NSModalResponseOK, NSSavePanel};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};
use objc2_uniform_type_identifiers::{UTType, UTTypePNG};
use std::{
    cell::RefCell,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// The frozen GPUI overlay sits at `NSPopUpWindowLevel` (101); the panel must be
/// one level above it to stay visible and key during explicit save.
const ABOVE_FROZEN_OVERLAY_LEVEL: isize = 102;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

thread_local! {
    /// The panel currently shown, retained until the app drains its outcome.
    static ACTIVE_PANEL: RefCell<Option<Retained<NSSavePanel>>> = const { RefCell::new(None) };
}

/// The single completion outcome, written by AppKit's main-thread handler.
static PANEL_OUTCOME: Mutex<Option<SavePanelOutcome>> = Mutex::new(None);

/// How one native PNG save panel ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SavePanelOutcome {
    /// The user chose this local file destination.
    Chosen(PathBuf),
    /// The user cancelled; nothing was written and this is not an error.
    Cancelled,
    /// The panel could not produce a usable destination.
    Failed(SavePanelError),
}

/// A native save panel that could not be shown or resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavePanelError {
    /// AppKit was asked off the main thread, where panel UI is unsafe.
    NotMainThread,
    /// Another panel is already shown; only one may own the output mutex.
    Busy,
    /// The panel reported OK without a file URL.
    MissingURL,
    /// The chosen URL was not a local file.
    NotFileURL,
    /// AppKit aborted the panel before the user answered.
    Aborted,
}
impl fmt::Display for SavePanelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotMainThread => "save panel must be shown on the main thread",
            Self::Busy => "a save panel is already open",
            Self::MissingURL => "save panel returned no file URL",
            Self::NotFileURL => "save panel returned a non-file URL",
            Self::Aborted => "save panel aborted before the user answered",
        })
    }
}
impl Error for SavePanelError {}

/// A configured native PNG save panel; [`Self::begin`] shows it asynchronously.
#[derive(Debug)]
pub struct PngSavePanel {
    panel: Retained<NSSavePanel>,
}

impl PngSavePanel {
    /// Creates the PNG save panel without showing it.
    ///
    /// # Errors
    /// Returns [`SavePanelError::NotMainThread`] off the main thread.
    pub fn try_new(suggested_name: &str) -> Result<Self, SavePanelError> {
        let marker = MainThreadMarker::new().ok_or(SavePanelError::NotMainThread)?;
        let panel = NSSavePanel::savePanel(marker);
        panel.setNameFieldStringValue(&NSString::from_str(suggested_name));
        // SAFETY: `UTTypePNG` is an immutable UniformTypeIdentifiers constant.
        let png_type: &UTType = unsafe { UTTypePNG };
        panel.setAllowedContentTypes(&NSArray::from_slice(&[png_type]));
        panel.setCanCreateDirectories(true);
        panel.setTitle(Some(&NSString::from_str("导出 PNG")));
        panel.setPrompt(Some(&NSString::from_str("保存")));
        panel.setLevel(ABOVE_FROZEN_OVERLAY_LEVEL);
        Ok(Self { panel })
    }

    /// Shows the panel and returns immediately; the outcome arrives later.
    ///
    /// # Errors
    /// Returns [`SavePanelError::NotMainThread`] off the main thread and
    /// [`SavePanelError::Busy`] while another panel is still shown.
    pub fn begin(&self) -> Result<(), SavePanelError> {
        if MainThreadMarker::new().is_none() {
            return Err(SavePanelError::NotMainThread);
        }
        ACTIVE_PANEL.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_some() {
                return Err(SavePanelError::Busy);
            }
            *slot = Some(self.panel.clone());
            Ok(())
        })?;
        let panel = self.panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            let outcome = if response == NSModalResponseOK {
                match chosen_path(&panel) {
                    Ok(path) => SavePanelOutcome::Chosen(path),
                    Err(error) => SavePanelOutcome::Failed(error),
                }
            } else if response == NSModalResponseCancel {
                SavePanelOutcome::Cancelled
            } else {
                // NSModalResponseAbort or an unknown response: no destination.
                SavePanelOutcome::Failed(SavePanelError::Aborted)
            };
            if let Ok(mut slot) = PANEL_OUTCOME.lock() {
                *slot = Some(outcome);
            }
        });
        self.panel.beginWithCompletionHandler(&handler);
        Ok(())
    }
}

/// Turns the panel's URL into the local file the user chose.
fn chosen_path(panel: &NSSavePanel) -> Result<PathBuf, SavePanelError> {
    let url = panel.URL().ok_or(SavePanelError::MissingURL)?;
    if !url.isFileURL() {
        return Err(SavePanelError::NotFileURL);
    }
    let path = url.path().ok_or(SavePanelError::MissingURL)?;
    Ok(PathBuf::from(path.to_string()))
}

/// Takes the completion outcome of the most recent panel, if the user answered.
///
/// Dropping the retained panel here guarantees AppKit already returned from its
/// completion handler before the native window is released. Returns `None` off
/// the main thread or while the panel is still open.
#[must_use]
pub fn take_save_panel_outcome() -> Option<SavePanelOutcome> {
    MainThreadMarker::new()?;
    let outcome = PANEL_OUTCOME.lock().ok()?.take()?;
    ACTIVE_PANEL.with(|slot| slot.borrow_mut().take());
    Some(outcome)
}

/// Cancels the panel the app is showing, if any.
///
/// A late completion outcome stays fenced by the app's job and is discarded.
/// When AppKit already stored an outcome, the panel is closed and only the app
/// has not drained it yet, so it is not cancelled a second time.
pub fn cancel_save_panel() {
    if MainThreadMarker::new().is_none() {
        return;
    }
    if PANEL_OUTCOME.lock().is_ok_and(|outcome| outcome.is_some()) {
        return;
    }
    // Clone out of the slot so a synchronously delivered completion cannot see
    // a borrowed slot while it stores its outcome.
    let panel = ACTIVE_PANEL.with(|slot| slot.borrow().clone());
    let Some(panel) = panel else {
        return;
    };
    // SAFETY: `cancel:` is an AppKit action taking one sender argument. A nil
    // sender is allowed because the implementation only uses the receiver.
    unsafe { panel.cancel(None) };
}

/// Writes encoded PNG bytes to the chosen file and confirms the byte count.
///
/// The PNG signature and byte budget are checked before `NSData` copies the
/// bytes into a Foundation allocation. `writeToURL:atomically:` reports actual
/// success; the written length is then verified, and a mismatch removes the
/// file we just wrote instead of leaving a partial PNG behind.
///
/// # Errors
/// Returns [`SaveFileError`] for a worker thread, non-PNG or over-budget bytes,
/// a non-UTF-8 path, an AppKit/disk write rejection, or a failed read-back.
pub fn write_png_to_file(path: &Path, png: &[u8]) -> Result<(), SaveFileError> {
    if MainThreadMarker::new().is_none() {
        return Err(SaveFileError::NotMainThread);
    }
    if !png.starts_with(PNG_SIGNATURE) {
        return Err(SaveFileError::InvalidPng);
    }
    if png.len() > MAX_PNG_BYTES {
        return Err(SaveFileError::Budget);
    }
    let path = path.to_str().ok_or(SaveFileError::Path)?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    let data = NSData::with_bytes(png);
    if !data.writeToURL_atomically(&url, true) {
        return Err(SaveFileError::WriteRejected);
    }
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.len() == png.len() as u64 => Ok(()),
        _ => {
            let _ = std::fs::remove_file(path);
            Err(SaveFileError::Verify)
        }
    }
}

/// A chosen PNG file was not confirmed written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveFileError {
    /// AppKit/Foundation was asked off the main thread.
    NotMainThread,
    /// The bytes are not a PNG, so no file was created.
    InvalidPng,
    /// The encoded bytes exceed V1's PNG budget.
    Budget,
    /// The chosen path is not representable as UTF-8.
    Path,
    /// `writeToURL:atomically:` reported failure.
    WriteRejected,
    /// The file did not read back with the expected byte count.
    Verify,
}
impl fmt::Display for SaveFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotMainThread => "PNG file writes must run on the main thread",
            Self::InvalidPng => "the bytes to write are not a PNG",
            Self::Budget => "the PNG exceeds the 128 MiB output budget",
            Self::Path => "the chosen path is not valid UTF-8",
            Self::WriteRejected => "AppKit rejected the PNG file write",
            Self::Verify => "the written PNG did not match the encoded bytes",
        })
    }
}
impl Error for SaveFileError {}
