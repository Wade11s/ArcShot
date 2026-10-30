//! One worker and an absolute, application-owned native completion deadline.
use arcshot_core::{CaptureFailure, CaptureTicket, CapturedFrame};
use std::{
    error::Error,
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

/// Safety deadline from submission, covering discovery and the first SCK invocation.
/// This is independent of the upstream timeout environment, not a latency target.
const NATIVE_DEADLINE: Duration = Duration::from_secs(5);

type CaptureResult = Result<CapturedFrame, CaptureFailure>;
#[derive(Debug)]
struct InFlight {
    ticket: CaptureTicket,
    started: Instant,
    receiver: Receiver<CaptureResult>,
    worker: thread::JoinHandle<()>,
}

/// A single native worker gate; timeout/panic permanently closes it for this process.
///
/// The coordinator keeps polling even when the session is cancelled. Dropping a job's
/// receiver discards any late image. An unconfirmed worker is never joined on the UI
/// thread or replaced with another native request.
#[derive(Debug, Default)]
pub struct CaptureTasks {
    in_flight: Option<InFlight>,
    terminal: bool,
}
impl CaptureTasks {
    /// Submits a native operation only when no previous request can still be running.
    ///
    /// The injected operation is the macOS capture boundary, not an internal mock.
    /// Pass the current monotonic instant immediately before submission.
    ///
    /// # Errors
    /// Rejects a busy/terminal gate, or failure to create the worker before any SCK call.
    pub fn submit(
        &mut self,
        ticket: CaptureTicket,
        now: Instant,
        capture: impl FnOnce() -> CaptureResult + Send + 'static,
    ) -> Result<(), CaptureSubmitError> {
        if self.terminal {
            return Err(CaptureSubmitError::RelaunchRequired);
        }
        if self.in_flight.is_some() {
            return Err(CaptureSubmitError::Busy);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("arcshot-capture".into())
            .spawn(move || {
                // This executable uses unwind panics. Only this worker boundary catches an
                // unexpected unwind: it becomes a terminal failure, never normal recovery.
                let result = panic::catch_unwind(AssertUnwindSafe(capture))
                    .unwrap_or(Err(CaptureFailure::WorkerPanicked));
                let _ = sender.send(result);
            })
            .map_err(|_| CaptureSubmitError::WorkerUnavailable)?;
        self.in_flight = Some(InFlight {
            ticket,
            started: now,
            receiver,
            worker,
        });
        Ok(())
    }

    /// Returns at most one result, or terminal uncertainty at the absolute deadline.
    ///
    /// A result observed after the deadline is not treated as proof that native work
    /// finished on time. The deadline is evaluated before inspecting the receive queue.
    pub fn poll(&mut self, now: Instant) -> Option<(CaptureTicket, CaptureResult)> {
        let flight = self.in_flight.as_ref()?;
        let expired = now.saturating_duration_since(flight.started) >= NATIVE_DEADLINE;
        if !expired && !flight.worker.is_finished() {
            return None;
        }
        let flight = self.in_flight.take()?;
        let result = if expired {
            // Dropping the handle detaches an unconfirmed worker. Never wait for it
            // on the UI thread, and never allow another native submission.
            Err(CaptureFailure::NativeUncertain)
        } else if flight.worker.join().is_err() {
            // is_finished established that joining cannot block on native work.
            Err(CaptureFailure::WorkerPanicked)
        } else {
            match flight.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                    Err(CaptureFailure::WorkerPanicked)
                }
            }
        };
        self.terminal = result
            .as_ref()
            .is_err_and(|error| error.requires_relaunch());
        Some((flight.ticket, result))
    }
}

/// Submission failed before starting any additional native operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSubmitError {
    /// A previous operation must complete first.
    Busy,
    /// Native state is uncertain and the process must be relaunched.
    RelaunchRequired,
    /// The OS could not create the worker; no request started.
    WorkerUnavailable,
}
impl fmt::Display for CaptureSubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "capture worker is still active",
            Self::RelaunchRequired => "capture requires application relaunch",
            Self::WorkerUnavailable => "could not create capture worker",
        })
    }
}
impl Error for CaptureSubmitError {}
