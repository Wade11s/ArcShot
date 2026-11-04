//! APP-ASYNC / APP-FRAME: the trusted-first-frame worker gate and admission.
//!
//! The scroll initial frame runs on the same bounded single-worker contract as
//! a region capture: at most one in-flight SCK request, an absolute deadline,
//! terminal uncertainty, and admission through the named-sRGB neutral frame
//! constructor against the fixed display/selection/scale.
use arcshot_app::{
    MAX_RAW_FRAMES, ScrollFrameQueue, ScrollFrameQueueError, ScrollFrameSubmitError,
    ScrollFrameTasks, admit_scroll_capture, may_submit_scroll_frame,
};
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureFailure, CaptureSession, CapturedFrame, PixelRect,
    ScreenGeometry, scroll_frame_geometry,
};
use std::time::{Duration, Instant};

/// One live ticket from a real session start; tickets are never invented.
fn ticket() -> arcshot_core::CaptureTicket {
    let mut session = CaptureSession::default();
    let effects = session.dispatch(CaptureAction::Start);
    let CaptureEffect::CheckPermission(ticket) = &effects[0] else {
        panic!("expected a permission check");
    };
    *ticket
}

fn region() -> ScreenGeometry {
    ScreenGeometry::try_new(1, 0.0, 0.0, 100.0, 50.0, 2.0).unwrap()
}

fn crop() -> PixelRect {
    PixelRect::try_new(20, 10, 60, 40).unwrap()
}

fn geometry() -> ScreenGeometry {
    scroll_frame_geometry(region(), crop()).unwrap()
}

/// Opaque BGRA bytes for one whole frame of the given pixel size.
fn opaque_bytes(width: u32, height: u32) -> Vec<u8> {
    vec![255; width as usize * height as usize * 4]
}

fn admitted_frame() -> CapturedFrame {
    CapturedFrame::try_from_srgb_bgra(geometry(), 60, 40, 60 * 4, opaque_bytes(60, 40)).unwrap()
}

/// A native capture result shaped like `arcshot_macos::NativeCapture`.
struct Native {
    width: u32,
    height: u32,
    stride: usize,
    bgra: Vec<u8>,
}

impl Native {
    fn valid() -> Self {
        Self {
            width: 60,
            height: 40,
            stride: 60 * 4,
            bgra: opaque_bytes(60, 40),
        }
    }
}

#[test]
fn app_async_13_single_in_flight_gate_reopens_only_after_a_known_result() {
    let mut tasks = ScrollFrameTasks::default();
    let ticket = ticket();
    let started = Instant::now();
    assert_eq!(
        tasks.submit(ticket, started, || Ok(admitted_frame())),
        Ok(())
    );
    assert_eq!(
        tasks.submit(ticket, started, || Ok(admitted_frame())),
        Err(ScrollFrameSubmitError::Busy)
    );
    // The worker finished long ago in real time; the deadline has not expired.
    std::thread::sleep(Duration::from_millis(100));
    let later = started + Duration::from_secs(4);
    let Some((done, result)) = tasks.poll(later) else {
        panic!("expected one finished result");
    };
    assert_eq!(done, ticket);
    assert!(result.is_ok());
    // A known result reopens the gate for the next scroll attempt.
    assert_eq!(tasks.submit(ticket, later, || Ok(admitted_frame())), Ok(()));
}

#[test]
fn app_async_14_absolute_deadline_is_terminal_and_forbids_resubmission() {
    let mut tasks = ScrollFrameTasks::default();
    let ticket = ticket();
    let started = Instant::now();
    let (sender, receiver) = std::sync::mpsc::channel::<()>();
    assert_eq!(
        tasks.submit(ticket, started, move || {
            // Block past the deadline like a stuck SCK request.
            let _ = receiver.recv();
            Ok(admitted_frame())
        }),
        Ok(())
    );
    let expired = started + Duration::from_secs(6);
    let Some((done, result)) = tasks.poll(expired) else {
        panic!("expected a terminal result at the deadline");
    };
    assert_eq!(done, ticket);
    assert_eq!(result, Err(CaptureFailure::NativeUncertain));
    // Native state is unconfirmed: no second scroll request in this process.
    assert_eq!(
        tasks.submit(ticket, expired, || Ok(admitted_frame())),
        Err(ScrollFrameSubmitError::RelaunchRequired)
    );
    drop(sender);
}

#[test]
fn app_async_15_worker_panic_is_terminal_native_uncertainty() {
    let mut tasks = ScrollFrameTasks::default();
    let ticket = ticket();
    let started = Instant::now();
    assert_eq!(
        tasks.submit(ticket, started, || {
            panic!("scroll frame worker unwound");
        }),
        Ok(())
    );
    // Give the worker a moment to unwind, then poll well before the deadline.
    std::thread::sleep(Duration::from_millis(100));
    let Some((done, result)) = tasks.poll(started + Duration::from_secs(1)) else {
        panic!("expected a terminal panic result");
    };
    assert_eq!(done, ticket);
    assert_eq!(result, Err(CaptureFailure::WorkerPanicked));
    assert_eq!(
        tasks.submit(ticket, started, || Ok(admitted_frame())),
        Err(ScrollFrameSubmitError::RelaunchRequired)
    );
}

#[test]
fn app_async_16_a_late_result_after_cancel_is_delivered_not_dropped_silently() {
    // Cancellation cannot stop a native request; the gate keeps polling until
    // the known result arrives and hands it to the core fence, which decides.
    let mut tasks = ScrollFrameTasks::default();
    let ticket = ticket();
    let started = Instant::now();
    assert_eq!(
        tasks.submit(ticket, started, || {
            std::thread::sleep(Duration::from_millis(50));
            Ok(admitted_frame())
        }),
        Ok(())
    );
    let _ = tasks.poll(started + Duration::from_millis(10));
    std::thread::sleep(Duration::from_millis(100));
    let Some((done, result)) = tasks.poll(started + Duration::from_secs(2)) else {
        panic!("expected the finished frame");
    };
    assert_eq!(done, ticket);
    assert!(result.is_ok());
}

#[test]
fn app_frame_03_valid_native_capture_admits_with_the_fixed_selection_geometry() {
    let native = Native::valid();
    let frame = admit_scroll_capture(
        region(),
        crop(),
        native.width,
        native.height,
        native.stride,
        native.bgra,
    )
    .expect("a well-formed capture admits");
    assert_eq!(frame.geometry(), geometry());
    assert_eq!(frame.geometry().pixel_size(), [60, 40]);
}

#[test]
fn app_frame_04_scroll_admission_rejects_malformed_dimensions_stride_and_bytes() {
    // Dimensions that disagree with the fixed selection crop.
    let bad = Native {
        width: 61,
        height: 40,
        stride: 61 * 4,
        bgra: vec![255; 61 * 40 * 4],
    };
    assert_eq!(
        admit_scroll_capture(
            region(),
            crop(),
            bad.width,
            bad.height,
            bad.stride,
            bad.bgra
        ),
        Err(CaptureFailure::InvalidFrame)
    );

    // A stride shorter than one pixel row.
    let bad = Native {
        stride: 8,
        ..Native::valid()
    };
    assert_eq!(
        admit_scroll_capture(
            region(),
            crop(),
            bad.width,
            bad.height,
            bad.stride,
            bad.bgra
        ),
        Err(CaptureFailure::InvalidFrame)
    );

    // A byte count that does not match stride times height.
    let mut bad = Native::valid();
    bad.bgra.pop();
    assert_eq!(
        admit_scroll_capture(
            region(),
            crop(),
            bad.width,
            bad.height,
            bad.stride,
            bad.bgra
        ),
        Err(CaptureFailure::InvalidFrame)
    );
}

// APP-ASYNC: the raw-frame queue applies backpressure instead of dropping.
#[test]
fn app_async_17_raw_frame_queue_caps_at_two_and_reports_backpressure() {
    let mut queue = ScrollFrameQueue::new();
    assert!(queue.is_empty());
    assert!(!queue.is_full());
    queue.push(admitted_frame()).expect("first frame queues");
    queue.push(admitted_frame()).expect("second frame queues");
    assert_eq!(queue.len(), MAX_RAW_FRAMES);
    assert!(queue.is_full());
    // A third frame is never silently dropped: the caller gets backpressure.
    assert_eq!(
        queue.push(admitted_frame()),
        Err(ScrollFrameQueueError::Backpressure)
    );
    assert_eq!(queue.len(), MAX_RAW_FRAMES);
    // FIFO order is preserved when verification drains the queue.
    assert!(queue.pop().is_some());
    assert!(queue.pop().is_some());
    assert!(queue.pop().is_none());
}

#[test]
fn app_async_18_submission_policy_requires_an_idle_gate_and_short_queue() {
    assert!(may_submit_scroll_frame(false, 0));
    assert!(may_submit_scroll_frame(false, 1));
    assert!(!may_submit_scroll_frame(false, MAX_RAW_FRAMES));
    assert!(!may_submit_scroll_frame(true, 0));
    assert!(!may_submit_scroll_frame(true, MAX_RAW_FRAMES));
}

#[test]
fn app_async_19_cancel_clears_the_queue_without_dropping_a_new_collection() {
    let mut queue = ScrollFrameQueue::new();
    queue.push(admitted_frame()).expect("queued");
    queue.clear();
    assert!(queue.is_empty());
    queue
        .push(admitted_frame())
        .expect("a later collection may queue");
    assert_eq!(queue.len(), 1);
}

#[test]
fn app_async_20_gate_plus_queue_allow_at_most_three_outstanding_frames() {
    let mut queue = ScrollFrameQueue::new();
    let mut in_flight = false;
    // One request occupies the single gate; two more may queue behind it.
    assert!(may_submit_scroll_frame(in_flight, queue.len()));
    in_flight = true;
    assert!(!may_submit_scroll_frame(in_flight, queue.len()));
    queue.push(admitted_frame()).expect("first queued");
    queue.push(admitted_frame()).expect("second queued");
    assert!(!may_submit_scroll_frame(in_flight, queue.len()));
    // Backpressure releases only after verification consumes a queued frame.
    assert!(queue.pop().is_some());
    assert!(!may_submit_scroll_frame(in_flight, queue.len()));
    in_flight = false;
    assert!(may_submit_scroll_frame(in_flight, queue.len()));
}

#[test]
fn app_frame_05_scroll_admission_requires_full_opacity_under_the_selection_geometry() {
    // One non-opaque pixel in the first row fails admission; the premultiplied
    // or transparent bytes never become a scroll document.
    let mut bytes = opaque_bytes(60, 40);
    bytes[3] = 254;
    assert_eq!(
        admit_scroll_capture(region(), crop(), 60, 40, 60 * 4, bytes),
        Err(CaptureFailure::InvalidFrame)
    );
}
