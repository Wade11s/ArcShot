//! One background compose/encode worker; clipboard publishing stays on the UI thread.
use arcshot_core::{OutputFailure, OutputJob};
use std::{
    error::Error,
    fmt,
    panic::{self, AssertUnwindSafe},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

type EncodeResult = Result<Vec<u8>, OutputFailure>;
#[derive(Debug)]
struct InFlight {
    job: OutputJob,
    receiver: Receiver<EncodeResult>,
    worker: thread::JoinHandle<()>,
}

/// A single-in-flight gate for one document's compose/encode task.
///
/// The job carries its document version so the coordinator can fence the
/// clipboard side effect after the worker returns.
#[derive(Debug, Default)]
pub struct OutputTasks {
    in_flight: Option<InFlight>,
}
impl OutputTasks {
    /// Starts the injected encode operation when no other job is in flight.
    ///
    /// # Errors
    /// Returns [`OutputSubmitError::Busy`] while a job is still running and
    /// [`OutputSubmitError::WorkerUnavailable`] if the thread cannot start.
    pub fn submit(
        &mut self,
        job: OutputJob,
        encode: impl FnOnce() -> EncodeResult + Send + 'static,
    ) -> Result<(), OutputSubmitError> {
        if self.in_flight.is_some() {
            return Err(OutputSubmitError::Busy);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("arcshot-output".into())
            .spawn(move || {
                // Only this worker boundary catches an unexpected unwind; a
                // failed encode keeps the document, unlike uncertain native state.
                let result = panic::catch_unwind(AssertUnwindSafe(encode))
                    .unwrap_or(Err(OutputFailure::Worker));
                let _ = sender.send(result);
            })
            .map_err(|_| OutputSubmitError::WorkerUnavailable)?;
        self.in_flight = Some(InFlight {
            job,
            receiver,
            worker,
        });
        Ok(())
    }

    /// Returns at most one finished job without blocking the UI thread.
    pub fn poll(&mut self) -> Option<(OutputJob, EncodeResult)> {
        let flight = self.in_flight.as_ref()?;
        if !flight.worker.is_finished() {
            return None;
        }
        let flight = self.in_flight.take()?;
        let result = if flight.worker.join().is_err() {
            Err(OutputFailure::Worker)
        } else {
            match flight.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => Err(OutputFailure::Worker),
            }
        };
        Some((flight.job, result))
    }

    /// Whether an encode job still owns the single slot.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.in_flight.is_some()
    }
}

/// Encode submission failed before any worker started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputSubmitError {
    /// Another encode job is still in flight.
    Busy,
    /// The OS could not create the worker.
    WorkerUnavailable,
}
impl fmt::Display for OutputSubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "output worker is still active",
            Self::WorkerUnavailable => "could not create output worker",
        })
    }
}
impl Error for OutputSubmitError {}
