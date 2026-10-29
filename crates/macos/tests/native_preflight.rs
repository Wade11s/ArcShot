//! Native preflight only. These tests never request permission or capture pixels.

use arcshot_macos::{
    CaptureError, DisplaySnapshot, application_active, builtin_display, capture_builtin,
    owned_visible_window_ids,
};

#[test]
fn app_frame_stale_display_is_rejected_before_capture() {
    let stale = DisplaySnapshot {
        id: 0,
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        scale: 1.0,
    };
    let error =
        capture_builtin(stale, Vec::new()).expect_err("invalid display must fail before SCK");
    assert!(matches!(error, CaptureError::Display));
    assert!(!error.requires_relaunch());
}

#[test]
fn native_display_preflight_reports_valid_2x_or_known_failure() {
    match builtin_display() {
        Ok(display) => {
            println!("display preflight: {display:?}");
            assert_eq!(display.scale, 2.0);
            assert!(display.id != 0 && display.width > 0.0 && display.height > 0.0);
        }
        Err(error) => {
            println!("display preflight unavailable: {error}");
            assert!(matches!(error, CaptureError::Display));
        }
    }
}

#[test]
fn native_request_error_is_terminal_but_known_failures_are_not() {
    assert!(CaptureError::Request("shareable content failed".into()).requires_relaunch());
    for error in [
        CaptureError::Display,
        CaptureError::Window,
        CaptureError::Frame,
        CaptureError::Budget,
        CaptureError::NotMainThread,
    ] {
        assert!(!error.requires_relaunch());
    }
}

#[test]
fn owned_window_enumeration_rejects_worker_thread() {
    let result = std::thread::spawn(owned_visible_window_ids)
        .join()
        .expect("worker should not panic");
    assert!(matches!(result, Err(CaptureError::NotMainThread)));
    assert!(
        !std::thread::spawn(application_active)
            .join()
            .expect("worker should not panic")
    );
}
