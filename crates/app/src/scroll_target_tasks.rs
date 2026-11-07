//! One bounded worker for the Scroll target probe, with an absolute deadline.
//!
//! The native probe can block on ScreenCaptureKit, so it runs on a worker and is
//! fenced exactly like a single-frame capture. A timeout is a terminal uncertain
//! native state: the app must not retry before a relaunch.
use arcshot_core::{CaptureTicket, ScrollTarget, ScrollTargetError};
use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

/// Safety deadline from submission, covering discovery and the SCK query.
const PROBE_DEADLINE: Duration = Duration::from_secs(5);

type ProbeResult = Result<ScrollTarget, ScrollTargetError>;

#[derive(Debug)]
struct InFlight {
    ticket: CaptureTicket,
    started: Instant,
    receiver: Receiver<ProbeResult>,
    worker: thread::JoinHandle<()>,
}

/// A single Scroll target probe gate; timeout/panic permanently closes it.
#[derive(Debug, Default)]
pub struct ScrollTargetTasks {
    in_flight: Option<InFlight>,
    terminal: bool,
}

impl ScrollTargetTasks {
    /// Submits the native probe only when no previous probe can still be running.
    ///
    /// # Errors
    /// Rejects a busy/terminal gate, or failure to create the worker before any
    /// native query started.
    pub fn submit(
        &mut self,
        ticket: CaptureTicket,
        now: Instant,
        probe: impl FnOnce() -> ProbeResult + Send + 'static,
    ) -> Result<(), ScrollTargetSubmitError> {
        if self.terminal {
            return Err(ScrollTargetSubmitError::RelaunchRequired);
        }
        if self.in_flight.is_some() {
            return Err(ScrollTargetSubmitError::Busy);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("arcshot-scroll-target".into())
            .spawn(move || {
                let result = panic::catch_unwind(AssertUnwindSafe(probe))
                    .unwrap_or(Err(ScrollTargetError::NativeUncertain));
                let _ = sender.send(result);
            })
            .map_err(|_| ScrollTargetSubmitError::WorkerUnavailable)?;
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
    /// A result observed after the deadline is not proof that native work finished
    /// on time, so it is discarded and the gate is closed for this process.
    pub fn poll(&mut self, now: Instant) -> Option<(CaptureTicket, ProbeResult)> {
        let flight = self.in_flight.as_ref()?;
        let expired = now.saturating_duration_since(flight.started) >= PROBE_DEADLINE;
        if !expired && !flight.worker.is_finished() {
            return None;
        }
        let flight = self.in_flight.take()?;
        let result = if expired {
            Err(ScrollTargetError::NativeUncertain)
        } else {
            match flight.worker.join() {
                Err(_) => Err(ScrollTargetError::NativeUncertain),
                Ok(()) => match flight.receiver.try_recv() {
                    Ok(result) => result,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                        Err(ScrollTargetError::NativeUncertain)
                    }
                },
            }
        };
        if matches!(result, Err(ScrollTargetError::NativeUncertain)) {
            self.terminal = true;
        }
        Some((flight.ticket, result))
    }
}

/// Submission failed before starting any additional native operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollTargetSubmitError {
    /// A previous probe must complete first.
    Busy,
    /// Native state is uncertain and the process must be relaunched.
    RelaunchRequired,
    /// The OS could not create the worker; no query started.
    WorkerUnavailable,
}

impl fmt::Display for ScrollTargetSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "scroll target probe is still active",
            Self::RelaunchRequired => "scroll target probe requires application relaunch",
            Self::WorkerUnavailable => "could not create scroll target worker",
        })
    }
}
impl std::error::Error for ScrollTargetSubmitError {}
