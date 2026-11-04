//! MAC-RECOVERY native preflight for explicit save. No panel is shown here;
//! the main-thread file roundtrip runs in the `--smoke-test` process.
use arcshot_macos::{
    PngSavePanel, SaveFileError, SavePanelError, cancel_save_panel, take_save_panel_outcome,
    write_png_to_file,
};
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

#[test]
fn mac_recovery_23_01_save_panel_and_file_write_refuse_worker_threads() {
    let panel_error = std::thread::spawn(|| PngSavePanel::try_new("ArcShot.png").err())
        .join()
        .expect("worker must not panic");
    assert_eq!(panel_error, Some(SavePanelError::NotMainThread));

    let path = std::env::temp_dir().join(format!("arcshot-worker-save-{}.png", std::process::id()));
    let worker_path = path.clone();
    let written = std::thread::spawn(move || write_png_to_file(&worker_path, PNG))
        .join()
        .expect("worker must not panic");
    assert!(matches!(written, Err(SaveFileError::NotMainThread)));
    assert!(!path.exists(), "a refused worker write must create no file");

    // Closing a panel and draining its outcome are main-thread operations; a
    // worker call is inert rather than an unsafe AppKit access.
    let outcome = std::thread::spawn(take_save_panel_outcome)
        .join()
        .expect("worker must not panic");
    assert!(outcome.is_none());
    std::thread::spawn(cancel_save_panel)
        .join()
        .expect("worker must not panic");
}
