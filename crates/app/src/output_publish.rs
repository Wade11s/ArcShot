//! One fenced PNG publication step shared by Finish and explicit Save.
//!
//! The encoder is the single compose/encode route for both actions; the target
//! carried by [`OutputJob`] decides the one side effect that may follow. A
//! save destination is held for exactly the snapshot the user chose it for.
use arcshot_core::{CapturedFrame, MAX_PNG_BYTES, OutputFailure, OutputJob, OutputTarget};
use std::path::{Path, PathBuf};

/// Composes and encodes one fenced snapshot through the shared image pipeline.
///
/// Both Finish and Save submit this same operation for the same document
/// version, so the clipboard and file paths reuse identical PNG bytes.
///
/// # Errors
/// Returns [`OutputFailure::ResourceLimit`] for the export budget and
/// [`OutputFailure::Encode`] for any compose/encode failure.
pub fn encode_document(frame: &CapturedFrame, job: OutputJob) -> Result<Vec<u8>, OutputFailure> {
    arcshot_image::compose_crop(frame, job.crop())
        .and_then(|composed| arcshot_image::encode_png(&composed))
        .map_err(|error| match error {
            arcshot_image::ComposeError::ResourceLimit => OutputFailure::ResourceLimit,
            _ => OutputFailure::Encode,
        })
}

/// A save destination chosen in the native panel, held for one output job.
///
/// Core never sees paths; the app keeps the chosen destination until the exact
/// snapshot it belongs to publishes or the session/panel ends.
#[derive(Debug, Default)]
pub struct PendingSave {
    destination: Option<(OutputJob, PathBuf)>,
}

impl PendingSave {
    /// Whether a destination is still waiting for its output job.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.destination.is_some()
    }

    /// Records the destination the user chose for exactly this job.
    pub fn remember(&mut self, job: OutputJob, path: PathBuf) {
        self.destination = Some((job, path));
    }

    /// Drops any destination without writing; used when the session ends.
    pub fn clear(&mut self) {
        self.destination = None;
    }

    /// Takes the destination only when this job is the one it was chosen for.
    fn take(&mut self, job: OutputJob) -> Option<PathBuf> {
        let (pending, path) = self.destination.take()?;
        (pending == job).then_some(path)
    }
}

/// The single side effect one fenced output job performs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputPublish {
    /// Finish: copy the PNG to the pasteboard.
    Clipboard,
    /// Explicit Save: write the PNG to the chosen file.
    SaveFile(PathBuf),
}

/// Publishes encoded bytes through exactly one injected native side effect.
///
/// The PNG byte cap is checked before either side effect. A save job without
/// the matching chosen destination performs no side effect at all, so a stale
/// job can never write a file or fall back to the clipboard.
///
/// # Errors
/// Returns [`OutputFailure::ResourceLimit`] above the PNG budget,
/// [`OutputFailure::SaveFile`] without a matching destination, or the native
/// failure reported by the injected side effect.
pub fn publish_output(
    job: OutputJob,
    png: &[u8],
    pending: &mut PendingSave,
    clipboard: impl FnOnce(&[u8]) -> Result<(), OutputFailure>,
    save: impl FnOnce(&Path, &[u8]) -> Result<(), OutputFailure>,
) -> Result<(), OutputFailure> {
    let publish = match job.target() {
        OutputTarget::Clipboard => OutputPublish::Clipboard,
        OutputTarget::SaveFile => {
            OutputPublish::SaveFile(pending.take(job).ok_or(OutputFailure::SaveFile)?)
        }
    };
    if png.len() > MAX_PNG_BYTES {
        return Err(OutputFailure::ResourceLimit);
    }
    match publish {
        OutputPublish::Clipboard => clipboard(png),
        OutputPublish::SaveFile(path) => save(&path, png),
    }
}
