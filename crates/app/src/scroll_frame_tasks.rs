//! One bounded worker for the Scroll trusted-initial-frame capture.
//!
//! The native SCK request can block, so it runs on a worker under the same
//! contract as a region capture: at most one in-flight request per process, an
//! absolute deadline, and a timeout/panic that permanently closes the gate
//! because the native request state can no longer be confirmed.
use arcshot_core::{
    CaptureFailure, CaptureTicket, CapturedFrame, PixelRect, ScreenGeometry, scroll_frame_geometry,
};
use std::{
    error::Error,
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

/// Safety deadline from submission, covering discovery and the SCK request.
const FRAME_DEADLINE: Duration = Duration::from_secs(5);

type FrameResult = Result<CapturedFrame, CaptureFailure>;

#[derive(Debug)]
struct InFlight {
    ticket: CaptureTicket,
    started: Instant,
    receiver: Receiver<FrameResult>,
    worker: thread::JoinHandle<()>,
}

/// A single Scroll frame request gate; timeout/panic permanently closes it.
#[derive(Debug, Default)]
pub struct ScrollFrameTasks {
    in_flight: Option<InFlight>,
    terminal: bool,
}

impl ScrollFrameTasks {
    /// Submits the native frame request only when no previous one can still run.
    ///
    /// # Errors
    /// Rejects a busy/terminal gate, or failure to create the worker before any
    /// native request started.
    pub fn submit(
        &mut self,
        ticket: CaptureTicket,
        now: Instant,
        capture: impl FnOnce() -> FrameResult + Send + 'static,
    ) -> Result<(), ScrollFrameSubmitError> {
        if self.terminal {
            return Err(ScrollFrameSubmitError::RelaunchRequired);
        }
        if self.in_flight.is_some() {
            return Err(ScrollFrameSubmitError::Busy);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("arcshot-scroll-frame".into())
            .spawn(move || {
                // A worker unwind is a terminal uncertain native state, never
                // normal recovery: the request may still be in flight.
                let result = panic::catch_unwind(AssertUnwindSafe(capture))
                    .unwrap_or(Err(CaptureFailure::WorkerPanicked));
                let _ = sender.send(result);
            })
            .map_err(|_| ScrollFrameSubmitError::WorkerUnavailable)?;
        self.in_flight = Some(InFlight {
            ticket,
            started: now,
            receiver,
            worker,
        });
        Ok(())
    }

    /// Returns at most one result, or terminal uncertainty at the deadline.
    ///
    /// A result observed after the deadline is not proof the native request
    /// finished on time, so it is discarded and the gate closes for this process.
    pub fn poll(&mut self, now: Instant) -> Option<(CaptureTicket, FrameResult)> {
        let flight = self.in_flight.as_ref()?;
        let expired = now.saturating_duration_since(flight.started) >= FRAME_DEADLINE;
        if !expired && !flight.worker.is_finished() {
            return None;
        }
        let flight = self.in_flight.take()?;
        let result = if expired {
            Err(CaptureFailure::NativeUncertain)
        } else {
            match flight.worker.join() {
                Err(_) => Err(CaptureFailure::WorkerPanicked),
                Ok(()) => match flight.receiver.try_recv() {
                    Ok(result) => result,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                        Err(CaptureFailure::WorkerPanicked)
                    }
                },
            }
        };
        if result
            .as_ref()
            .is_err_and(|error| error.requires_relaunch())
        {
            self.terminal = true;
        }
        Some((flight.ticket, result))
    }

    /// Whether a frame request still owns the single slot.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.in_flight.is_some()
    }
}

/// Submission failed before starting any additional native operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollFrameSubmitError {
    /// A previous frame request must complete first.
    Busy,
    /// Native state is uncertain and the process must be relaunched.
    RelaunchRequired,
    /// The OS could not create the worker; no request started.
    WorkerUnavailable,
}

impl fmt::Display for ScrollFrameSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "scroll frame request is still active",
            Self::RelaunchRequired => "scroll frame capture requires application relaunch",
            Self::WorkerUnavailable => "could not create scroll frame worker",
        })
    }
}
impl Error for ScrollFrameSubmitError {}

/// Admits one native capture as the trusted Scroll frame for a fixed crop.
///
/// The frame geometry comes from the shared `scroll_frame_geometry` conversion
/// so the captured frame and the authoritative expectation agree by
/// construction; the neutral constructor then enforces dimensions, stride, byte
/// count, budget and full opacity before the frame can open collection.
///
/// # Errors
/// Returns [`CaptureFailure::InvalidFrame`] when the bytes fail the neutral
/// frame contract and [`CaptureFailure::ResourceLimit`] when the conversion or
/// admission exceeds the budget.
pub fn admit_scroll_capture(
    region: ScreenGeometry,
    crop: PixelRect,
    width: u32,
    height: u32,
    stride: usize,
    bgra: Vec<u8>,
) -> Result<CapturedFrame, CaptureFailure> {
    let geometry =
        scroll_frame_geometry(region, crop).map_err(|_| CaptureFailure::ResourceLimit)?;
    CapturedFrame::try_from_srgb_bgra(geometry, width, height, stride, bgra).map_err(|error| {
        match error {
            arcshot_core::FrameError::Budget => CaptureFailure::ResourceLimit,
            _ => CaptureFailure::InvalidFrame,
        }
    })
}
