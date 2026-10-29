//! APP-ASYNC: actual worker boundary with an injected capture operation and clock.
use arcshot_app::CaptureTasks;
use arcshot_core::{CaptureAction, CaptureEffect, CaptureFailure, CaptureSession};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn ticket() -> arcshot_core::CaptureTicket {
    let mut s = CaptureSession::default();
    let CaptureEffect::CheckPermission(ticket) = s.dispatch(CaptureAction::Start)[0] else {
        panic!("missing preflight");
    };
    ticket
}
#[test]
fn app_async_05_absolute_deadline_is_independent_of_sck_environment() {
    let t = ticket();
    let now = Instant::now();
    let mut tasks = CaptureTasks::default();
    let (release, wait) = mpsc::channel();
    tasks
        .submit(t, now, move || {
            let _ = wait.recv();
            Err(CaptureFailure::InvalidFrame)
        })
        .unwrap();
    assert_eq!(
        tasks.poll(now + Duration::from_secs(5)),
        Some((t, Err(CaptureFailure::NativeUncertain)))
    );
    assert!(
        tasks
            .submit(t, now, || Err(CaptureFailure::InvalidFrame))
            .is_err()
    );
    release.send(()).unwrap();
    assert!(tasks.poll(now + Duration::from_secs(6)).is_none());
}

fn receive(
    tasks: &mut CaptureTasks,
    now: Instant,
) -> (
    arcshot_core::CaptureTicket,
    Result<arcshot_core::CapturedFrame, CaptureFailure>,
) {
    for _ in 0..1000 {
        if let Some(result) = tasks.poll(now) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("worker did not report within test deadline");
}
#[test]
fn app_async_06_worker_panic_is_terminal_not_a_retryable_capture_failure() {
    let mut tasks = CaptureTasks::default();
    let t = ticket();
    let now = Instant::now();
    tasks
        .submit(t, now, || panic!("injected capture worker unwind"))
        .unwrap();
    assert_eq!(
        receive(&mut tasks, now),
        (t, Err(CaptureFailure::WorkerPanicked))
    );
    assert!(
        tasks
            .submit(t, now, || Err(CaptureFailure::InvalidFrame))
            .is_err()
    );
}
#[test]
fn app_async_07_single_inflight_gate_reopens_only_after_a_known_result() {
    let mut tasks = CaptureTasks::default();
    let t = ticket();
    let now = Instant::now();
    let (release, wait) = mpsc::channel();
    tasks
        .submit(t, now, move || {
            let _ = wait.recv();
            Err(CaptureFailure::InvalidFrame)
        })
        .unwrap();
    assert!(
        tasks
            .submit(t, now, || Err(CaptureFailure::InvalidFrame))
            .is_err()
    );
    release.send(()).unwrap();
    assert_eq!(
        receive(&mut tasks, now),
        (t, Err(CaptureFailure::InvalidFrame))
    );
    assert!(
        tasks
            .submit(t, now, || Err(CaptureFailure::InvalidFrame))
            .is_ok()
    );
    assert_eq!(
        receive(&mut tasks, now),
        (t, Err(CaptureFailure::InvalidFrame))
    );
}
