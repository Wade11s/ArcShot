//! APP-EFFECT/APP-ASYNC: encode worker boundary and single in-flight output.
use arcshot_app::OutputTasks;
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureSession, CapturedFrame, OutputFailure, OutputJob,
    ScreenGeometry,
};
use std::time::Duration;

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

fn output_job() -> OutputJob {
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
    session.dispatch(CaptureAction::SelectRegion {
        ticket,
        from: [10.0, 5.0],
        to: [40.0, 25.0],
    });
    let CaptureEffect::ComposeOutput(job) = session.dispatch(CaptureAction::Finish { ticket })[0]
    else {
        panic!("missing output job");
    };
    job
}

fn receive(tasks: &mut OutputTasks) -> (OutputJob, Result<Vec<u8>, OutputFailure>) {
    for _ in 0..1000 {
        if let Some(result) = tasks.poll() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("encode worker did not report within test deadline");
}

#[test]
fn app_effect_15_injected_encode_result_is_delivered_once_with_its_job() {
    let job = output_job();
    let mut tasks = OutputTasks::default();
    tasks.submit(job, || Ok(vec![1, 2, 3])).unwrap();
    assert!(tasks.busy());
    assert_eq!(receive(&mut tasks), (job, Ok(vec![1, 2, 3])));
    assert!(!tasks.busy());
    assert!(tasks.poll().is_none());
}

#[test]
fn app_effect_16_encode_panic_is_a_retryable_output_worker_failure() {
    let job = output_job();
    let mut tasks = OutputTasks::default();
    tasks
        .submit(job, || panic!("injected encode worker unwind"))
        .unwrap();
    assert_eq!(receive(&mut tasks), (job, Err(OutputFailure::Worker)));
    // Unlike an uncertain native request, a failed encode does not close the gate.
    tasks.submit(job, || Err(OutputFailure::Encode)).unwrap();
    assert_eq!(receive(&mut tasks), (job, Err(OutputFailure::Encode)));
}

#[test]
fn app_effect_17_second_output_job_is_rejected_while_one_is_in_flight() {
    let job = output_job();
    let mut tasks = OutputTasks::default();
    let (release, wait) = std::sync::mpsc::channel();
    tasks
        .submit(job, move || {
            let _ = wait.recv();
            Ok(vec![0])
        })
        .unwrap();
    assert!(tasks.submit(job, || Ok(vec![1])).is_err());
    release.send(()).unwrap();
    assert_eq!(receive(&mut tasks), (job, Ok(vec![0])));
    assert!(tasks.submit(job, || Ok(vec![1])).is_ok());
    assert_eq!(receive(&mut tasks), (job, Ok(vec![1])));
}
