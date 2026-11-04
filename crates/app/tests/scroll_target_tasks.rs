//! APP-ASYNC: the bounded Scroll target probe worker and its terminal deadline.
use arcshot_app::{ScrollTargetSubmitError, ScrollTargetTasks};
use arcshot_core::{CaptureTicket, ScrollTargetError};
use std::{
    sync::mpsc,
    thread::sleep,
    time::{Duration, Instant},
};

fn ticket() -> CaptureTicket {
    let mut session = arcshot_core::CaptureSession::default();
    let arcshot_core::CaptureEffect::CheckPermission(ticket) =
        session.dispatch(arcshot_core::CaptureAction::Start)[0]
    else {
        panic!("missing preflight");
    };
    ticket
}

#[test]
fn app_async_10_absolute_deadline_is_independent_of_sck_environment() {
    let ticket = ticket();
    let now = Instant::now();
    let mut tasks = ScrollTargetTasks::default();
    let (release, wait) = mpsc::channel();
    tasks
        .submit(ticket, now, move || {
            let _ = wait.recv();
            Err(ScrollTargetError::NotFrontmost)
        })
        .unwrap();
    assert_eq!(
        tasks.poll(now + Duration::from_secs(5)),
        Some((ticket, Err(ScrollTargetError::NativeUncertain)))
    );
    // A timeout is terminal: no in-session restart before a relaunch.
    assert!(matches!(
        tasks.submit(ticket, now, || Err(ScrollTargetError::NotFrontmost)),
        Err(ScrollTargetSubmitError::RelaunchRequired)
    ));
    release.send(()).unwrap();
    assert!(tasks.poll(now + Duration::from_secs(6)).is_none());
}

#[test]
fn app_async_11_known_target_failure_is_not_terminal() {
    let ticket = ticket();
    let now = Instant::now();
    let mut tasks = ScrollTargetTasks::default();
    tasks
        .submit(ticket, now, || Err(ScrollTargetError::NotFrontmost))
        .unwrap();
    sleep(Duration::from_millis(20));
    assert_eq!(
        tasks.poll(now + Duration::from_millis(20)),
        Some((ticket, Err(ScrollTargetError::NotFrontmost)))
    );
    // A known refusal keeps the gate open for a later manual attempt.
    tasks
        .submit(ticket, now, || Err(ScrollTargetError::NoTarget))
        .unwrap();
    sleep(Duration::from_millis(20));
    assert_eq!(
        tasks.poll(now + Duration::from_millis(40)),
        Some((ticket, Err(ScrollTargetError::NoTarget)))
    );
}

#[test]
fn app_async_12_single_inflight_gate_reopens_only_after_a_known_result() {
    let ticket = ticket();
    let now = Instant::now();
    let mut tasks = ScrollTargetTasks::default();
    let (release, wait) = mpsc::channel();
    tasks
        .submit(ticket, now, move || {
            let _ = wait.recv();
            Err(ScrollTargetError::AmbiguousTarget)
        })
        .unwrap();
    assert!(matches!(
        tasks.submit(ticket, now, || Err(ScrollTargetError::NoTarget)),
        Err(ScrollTargetSubmitError::Busy)
    ));
    release.send(()).unwrap();
    sleep(Duration::from_millis(20));
    assert_eq!(
        tasks.poll(now + Duration::from_millis(20)),
        Some((ticket, Err(ScrollTargetError::AmbiguousTarget)))
    );
    tasks
        .submit(ticket, now, || Err(ScrollTargetError::NoTarget))
        .unwrap();
    sleep(Duration::from_millis(20));
    assert_eq!(
        tasks.poll(now + Duration::from_millis(40)),
        Some((ticket, Err(ScrollTargetError::NoTarget)))
    );
}
