//! APP-EFFECT: one fenced PNG side effect — clipboard or explicit file, never both.
use arcshot_app::{PendingSave, encode_document, publish_output};
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureSession, CaptureTicket, CapturedFrame, MAX_PNG_BYTES,
    OutputFailure, OutputJob, OutputTarget, ScreenGeometry,
};
use std::path::{Path, PathBuf};

fn screen() -> ScreenGeometry {
    ScreenGeometry::try_new(1, 0.0, 0.0, 100.0, 50.0, 2.0).unwrap()
}

fn frame() -> CapturedFrame {
    let [w, h] = screen().pixel_size();
    CapturedFrame::try_from_srgb_bgra(
        screen(),
        w,
        h,
        w as usize * 4,
        vec![255; w as usize * h as usize * 4],
    )
    .unwrap()
}

fn frozen() -> (CaptureSession, CaptureTicket) {
    let mut session = CaptureSession::default();
    let CaptureEffect::CheckPermission(ticket) = session.dispatch(CaptureAction::Start)[0] else {
        panic!("missing preflight");
    };
    session.dispatch(CaptureAction::PermissionChecked {
        ticket,
        available: true,
    });
    session.dispatch(CaptureAction::Completed {
        ticket,
        result: Ok(frame()),
    });
    (session, ticket)
}

/// A frozen document with the 2x selection (10,5)-(40,25) at version 1.
fn editing() -> (CaptureSession, CaptureTicket) {
    let (mut session, ticket) = frozen();
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    (session, ticket)
}

fn clipboard_job() -> OutputJob {
    let (mut session, ticket) = editing();
    let CaptureEffect::ComposeOutput(job) = session.dispatch(CaptureAction::Finish { ticket })[0]
    else {
        panic!("missing clipboard job");
    };
    job
}

fn save_job_with_selection(from: [f64; 2], to: [f64; 2]) -> OutputJob {
    let (mut session, ticket) = frozen();
    session.dispatch(CaptureAction::SelectRegion { ticket, from, to });
    let CaptureEffect::RequestSavePanel(job) = session.dispatch(CaptureAction::Save { ticket })[0]
    else {
        panic!("missing save panel request");
    };
    job
}

fn save_job() -> OutputJob {
    save_job_with_selection([10.0, 5.0], [40.0, 25.0])
}

fn path() -> PathBuf {
    PathBuf::from("/tmp/arcshot-app-effect.png")
}

#[test]
fn app_effect_22_a_save_destination_is_used_by_exactly_its_job() {
    let save = save_job();
    assert_eq!(save.target(), OutputTarget::SaveFile);
    let mut pending = PendingSave::default();
    pending.remember(save, path());

    let mut clipboard_calls = 0;
    let mut file_calls = Vec::new();
    let published = publish_output(
        save,
        &[1, 2, 3],
        &mut pending,
        |_| {
            clipboard_calls += 1;
            Ok(())
        },
        |target: &Path, png| {
            file_calls.push((target.to_path_buf(), png.to_vec()));
            Ok(())
        },
    );
    assert_eq!(published, Ok(()));
    assert_eq!(clipboard_calls, 0);
    assert_eq!(file_calls, vec![(path(), vec![1, 2, 3])]);
    assert!(!pending.is_pending());

    // The same version is retryable, but the destination is single-shot.
    assert_eq!(
        publish_output(save, &[1, 2, 3], &mut pending, |_| Ok(()), |_, _| Ok(())),
        Err(OutputFailure::SaveFile)
    );
}

#[test]
fn app_effect_23_a_changed_document_version_never_writes_the_old_destination() {
    let save = save_job();
    let mut pending = PendingSave::default();
    pending.remember(save, path());

    // A newer selection bumps the document version; the old snapshot is stale.
    let newer = save_job_with_selection([0.0, 0.0], [20.0, 20.0]);
    assert_ne!(newer, save);
    let published = publish_output(
        newer,
        &[1, 2, 3],
        &mut pending,
        |_| panic!("a save job must never reach the clipboard"),
        |_, _| panic!("a stale destination must never write a file"),
    );
    assert_eq!(published, Err(OutputFailure::SaveFile));
    assert!(!pending.is_pending());
}

#[test]
fn app_effect_24_a_clipboard_job_never_writes_a_file_or_needs_a_destination() {
    let finish = clipboard_job();
    assert_eq!(finish.target(), OutputTarget::Clipboard);
    let mut pending = PendingSave::default();
    pending.remember(save_job(), path());

    let mut clipboard_bytes = None;
    let published = publish_output(
        finish,
        &[9, 8, 7],
        &mut pending,
        |png| {
            clipboard_bytes = Some(png.to_vec());
            Ok(())
        },
        |_, _| panic!("clipboard output must not write a file"),
    );
    assert_eq!(published, Ok(()));
    assert_eq!(clipboard_bytes, Some(vec![9, 8, 7]));
    // The unrelated save destination was not consumed or written.
    assert!(pending.is_pending());
}

#[test]
fn app_effect_25_one_snapshot_yields_identical_png_bytes_for_both_actions() {
    let clipboard_bytes = encode_document(&frame(), clipboard_job()).unwrap();
    let save_bytes = encode_document(&frame(), save_job()).unwrap();
    assert_eq!(clipboard_bytes, save_bytes);
    assert_eq!(&save_bytes[..8], b"\x89PNG\r\n\x1a\n");
    // IHDR follows the signature and length/type; the crop is 60x40 physical.
    assert_eq!(
        u32::from_be_bytes(save_bytes[16..20].try_into().unwrap()),
        60
    );
    assert_eq!(
        u32::from_be_bytes(save_bytes[20..24].try_into().unwrap()),
        40
    );
    // The same version reuses one encoding route rather than a second encoder.
    assert_eq!(encode_document(&frame(), save_job()).unwrap(), save_bytes);
}

#[test]
fn app_effect_26_over_budget_png_bytes_are_rejected_before_any_side_effect() {
    let save = save_job();
    let mut pending = PendingSave::default();
    pending.remember(save, path());
    let oversized = vec![0u8; MAX_PNG_BYTES + 1];
    let published = publish_output(
        save,
        &oversized,
        &mut pending,
        |_| panic!("a save job must never reach the clipboard"),
        |_, _| panic!("an over-budget PNG must never reach the file"),
    );
    assert_eq!(published, Err(OutputFailure::ResourceLimit));
    assert!(!pending.is_pending());
}
