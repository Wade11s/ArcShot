//! Authority for capture preparation, cancellation and native uncertainty.
use crate::{CapturedFrame, Document, PixelRect, Selection};
use std::{error::Error, fmt};

/// Identifies one capture attempt across permission, worker and window effects.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CaptureTicket(u64);

/// Observable capture lifecycle, independent of windows and system I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePhase {
    /// No capture or native request is active.
    Resident,
    /// A screen-recording preflight is outstanding.
    CheckingPermission,
    /// A bounded native request is being prepared or processed.
    Preparing,
    /// Cancelled; a native request must safely finish before another can start.
    Draining,
    /// An accepted immutable frame may be displayed.
    Frozen,
    /// Native state is uncertain; only application relaunch permits capture again.
    RelaunchRequired,
}

/// Semantic inputs and typed worker results.
#[derive(Debug)]
pub enum CaptureAction {
    /// Starts or safely restarts a capture.
    Start,
    /// Reports the screen-recording preflight for this attempt.
    PermissionChecked {
        /// Attempt that requested the check.
        ticket: CaptureTicket,
        /// Actual preflight, never the user's intended setting.
        available: bool,
    },
    /// Reports one terminal worker result.
    Completed {
        /// Attempt that owns this result.
        ticket: CaptureTicket,
        /// Normalized frame or classified failure.
        result: Result<CapturedFrame, CaptureFailure>,
    },
    /// Cancels without output, retaining an in-flight native safety fence.
    Cancel,
    /// Cancels only the attempt whose overlay produced this input.
    CancelAttempt(CaptureTicket),
    /// Replaces the region selection from a completed pointer drag, fenced by
    /// the overlay's attempt ticket. Zero-size or invalid drags change nothing.
    SelectRegion {
        /// Attempt whose overlay produced the drag.
        ticket: CaptureTicket,
        /// Drag start in display-local logical points.
        from: [f64; 2],
        /// Drag end in display-local logical points.
        to: [f64; 2],
    },
    /// Finishes through Enter or the toolbar button when a valid selection exists.
    Finish {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
    },
    /// Finishes on a double-click only when the point lies inside the selection.
    FinishAt {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Double-click point in display-local logical points.
        point: [f64; 2],
    },
    /// Reports the result of one fenced composition/encode/clipboard job.
    OutputCompleted {
        /// Exact document version and crop the job was composed from.
        job: OutputJob,
        /// `Ok` only after the clipboard reported success.
        result: Result<(), OutputFailure>,
    },
    /// Invalidates the built-in screen; never migrates to another display.
    DisplayInvalidated,
    /// The shared output/save mutex contract (I-10): while busy with output
    /// processing or an open save dialog, `Start` from the capture hotkey or
    /// menu is ignored. Output slices own setting and releasing it.
    SetOutputBusy {
        /// Whether an output action or save dialog is currently in flight.
        busy: bool,
    },
}

/// Tasks to execute outside the pure core, guarded by their attempt ticket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureEffect {
    /// Preflight only Screen Recording.
    CheckPermission(CaptureTicket),
    /// Launch at most one bounded native capture.
    Capture(CaptureTicket),
    /// Present the frame only if this ticket is still current.
    PresentFrozen(CaptureTicket),
    /// Close and release any preparing/frozen capture interface.
    DismissOverlay,
    /// Offer Settings, Recheck and Cancel, without attributing a denial reason.
    PermissionUnavailable,
    /// Show a classified failure; uncertainty requires relaunch.
    ReportFailure(CaptureFailure),
    /// Compose and encode exactly this document snapshot outside the UI thread.
    ComposeOutput(OutputJob),
    /// Play the short completion sound after successful clipboard output.
    PlayCompletionSound,
    /// Report an output failure without discarding the document.
    ReportOutputFailure(OutputFailure),
}

/// One version-fenced output job: the document may not change before publishing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputJob {
    ticket: CaptureTicket,
    version: u64,
    crop: PixelRect,
}
impl OutputJob {
    /// Overlay attempt that owns the document.
    #[must_use]
    pub const fn ticket(self) -> CaptureTicket {
        self.ticket
    }
    /// Document version the job was composed from.
    #[must_use]
    pub const fn version(self) -> u64 {
        self.version
    }
    /// Physical crop inside the immutable base image.
    #[must_use]
    pub const fn crop(self) -> PixelRect {
        self.crop
    }
}

/// Output failures that keep the frozen document available for retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFailure {
    /// The crop could not be composed within the pixel/memory budget.
    ResourceLimit,
    /// Composition or PNG encoding failed.
    Encode,
    /// The clipboard rejected the PNG or did not expose the PNG type.
    Clipboard,
    /// The encode worker could not run or unwound.
    Worker,
    /// A previous encode job still owns the single slot.
    Busy,
}
impl OutputFailure {
    fn message(self) -> &'static str {
        match self {
            Self::ResourceLimit => "所选区域超出导出尺寸/内存预算，未产生输出，可缩小选区后重试",
            Self::Encode => "合成或 PNG 编码失败，未产生输出，可重试或取消",
            Self::Clipboard => "剪贴板未确认写入 PNG，未报告成功；选区已保留，可重试",
            Self::Worker => "编码工作线程未能完成，未产生输出，可重试",
            Self::Busy => "上一个合成/编码任务仍在结束，未产生输出，请稍后重试",
        }
    }
}
impl fmt::Display for OutputFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}
impl Error for OutputFailure {}

/// Recovery classes shared by the coordinator and pure capture authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureFailure {
    /// The built-in screen disappeared or its geometry/scale changed.
    DisplayInvalid,
    /// The normalized frame did not meet the neutral frame contract.
    InvalidFrame,
    /// Admission or allocation exceeded the working budget.
    ResourceLimit,
    /// An SCK call failed or its completion deadline expired.
    NativeUncertain,
    /// A capture worker unwound before confirming native completion.
    WorkerPanicked,
    /// A worker could not be launched; no native request started.
    WorkerUnavailable,
}
impl CaptureFailure {
    /// Distinguishes process-terminal uncertainty from safely retryable failures.
    #[must_use]
    pub const fn requires_relaunch(self) -> bool {
        matches!(self, Self::NativeUncertain | Self::WorkerPanicked)
    }
}

impl fmt::Display for CaptureFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DisplayInvalid => "built-in display invalidated",
            Self::InvalidFrame => "capture frame failed admission",
            Self::ResourceLimit => "capture exceeds resource budget",
            Self::NativeUncertain => "native request state is uncertain",
            Self::WorkerPanicked => "capture worker panicked",
            Self::WorkerUnavailable => "capture worker could not be started",
        })
    }
}
impl Error for CaptureFailure {}

#[derive(Debug)]
enum State {
    Resident,
    Checking(CaptureTicket),
    Preparing(CaptureTicket),
    Draining {
        ticket: CaptureTicket,
        restart: bool,
    },
    Frozen {
        ticket: CaptureTicket,
        document: Document,
    },
    Relaunch,
}

/// The sole capture authority; contains no system handles, clocks or I/O.
#[derive(Debug)]
pub struct CaptureSession {
    epoch: u64,
    state: State,
    output_mutex: bool,
}
impl Default for CaptureSession {
    fn default() -> Self {
        Self {
            epoch: 0,
            state: State::Resident,
            output_mutex: false,
        }
    }
}

impl CaptureSession {
    /// Observes the lifecycle without exposing mutable document state.
    #[must_use]
    pub const fn phase(&self) -> CapturePhase {
        match self.state {
            State::Resident => CapturePhase::Resident,
            State::Checking(_) => CapturePhase::CheckingPermission,
            State::Preparing(_) => CapturePhase::Preparing,
            State::Draining { .. } => CapturePhase::Draining,
            State::Frozen { .. } => CapturePhase::Frozen,
            State::Relaunch => CapturePhase::RelaunchRequired,
        }
    }

    /// Whether the shared output/save mutex currently suppresses `Start`.
    #[must_use]
    pub const fn output_busy(&self) -> bool {
        self.output_mutex
    }

    /// Borrows the accepted immutable frame; cancelled/terminal sessions expose none.
    #[must_use]
    pub fn frame(&self) -> Option<&CapturedFrame> {
        self.document().map(Document::frame)
    }

    /// Borrows the frozen document, including its selection and version.
    #[must_use]
    pub fn document(&self) -> Option<&Document> {
        match &self.state {
            State::Frozen { document, .. } => Some(document),
            _ => None,
        }
    }

    /// Fences publishing encoded output immediately before the clipboard side effect.
    ///
    /// The job must belong to the current attempt, match the current document
    /// version and crop, and remain inside an active output operation.
    #[must_use]
    pub fn may_publish(&self, job: OutputJob) -> bool {
        let State::Frozen { ticket, document } = &self.state else {
            return false;
        };
        self.output_mutex && job_matches(*ticket, document, job)
    }

    /// Fences window presentation immediately before a UI side effect.
    #[must_use]
    pub fn may_present(&self, ticket: CaptureTicket) -> bool {
        matches!(self.state,State::Frozen{ticket:current,..} if current==ticket)
    }

    /// Fences worker submission immediately before a native side effect.
    #[must_use]
    pub fn may_capture(&self, ticket: CaptureTicket) -> bool {
        matches!(self.state,State::Preparing(current) if current==ticket)
    }

    /// Applies a semantic action and returns explicit effects for the app coordinator.
    pub fn dispatch(&mut self, action: CaptureAction) -> Vec<CaptureEffect> {
        match action {
            // The shared output/save mutex ignores the capture hotkey and the
            // menu Start alike; output slices release it explicitly.
            CaptureAction::Start if self.output_mutex => vec![],
            CaptureAction::Start => match self.state {
                State::Relaunch => vec![CaptureEffect::ReportFailure(
                    CaptureFailure::NativeUncertain,
                )],
                State::Preparing(ticket) | State::Draining { ticket, .. } => {
                    self.state = State::Draining {
                        ticket,
                        restart: true,
                    };
                    vec![CaptureEffect::DismissOverlay]
                }
                State::Frozen { .. } => {
                    let mut effects = vec![CaptureEffect::DismissOverlay];
                    effects.extend(self.begin());
                    effects
                }
                State::Checking(_) => vec![],
                State::Resident => self.begin(),
            },
            CaptureAction::PermissionChecked { ticket, available } => {
                if !matches!(self.state,State::Checking(current) if current==ticket) {
                    return vec![];
                }
                if available {
                    self.state = State::Preparing(ticket);
                    vec![CaptureEffect::Capture(ticket)]
                } else {
                    self.state = State::Resident;
                    vec![CaptureEffect::PermissionUnavailable]
                }
            }
            CaptureAction::Completed { ticket, result } => self.complete(ticket, result),
            CaptureAction::CancelAttempt(ticket) => {
                let current = match self.state {
                    State::Checking(current)
                    | State::Preparing(current)
                    | State::Frozen {
                        ticket: current, ..
                    } => current == ticket,
                    // A draining ticket owns the worker result, not input from
                    // its already-dismissed overlay. A queued restart must survive.
                    State::Resident | State::Relaunch | State::Draining { .. } => false,
                };
                if current {
                    self.dispatch(CaptureAction::Cancel)
                } else {
                    vec![]
                }
            }
            CaptureAction::SelectRegion { ticket, from, to } => {
                let State::Frozen {
                    ticket: current,
                    document,
                } = &mut self.state
                else {
                    return vec![];
                };
                if *current != ticket || self.output_mutex {
                    return vec![];
                }
                let Ok(selection) = Selection::from_drag(from, to) else {
                    return vec![];
                };
                // An out-of-bounds or over-budget drag leaves the prior selection.
                let _ = document.select(selection);
                vec![]
            }
            CaptureAction::Finish { ticket } => self.finish(ticket, None),
            CaptureAction::FinishAt { ticket, point } => self.finish(ticket, Some(point)),
            CaptureAction::OutputCompleted { job, result } => {
                let matches_job = match &self.state {
                    State::Frozen { ticket, document } => job_matches(*ticket, document, job),
                    _ => false,
                };
                // Stale or duplicated results never publish or end another session.
                if !matches_job || !self.output_mutex {
                    return vec![];
                }
                self.output_mutex = false;
                match result {
                    Ok(()) => {
                        self.state = State::Resident;
                        vec![
                            CaptureEffect::DismissOverlay,
                            CaptureEffect::PlayCompletionSound,
                        ]
                    }
                    Err(error) => vec![CaptureEffect::ReportOutputFailure(error)],
                }
            }
            CaptureAction::Cancel | CaptureAction::DisplayInvalidated => {
                // A cancelled output job may still finish; its fence must fail.
                self.output_mutex = false;
                let invalid = matches!(action, CaptureAction::DisplayInvalidated);
                self.state = match self.state {
                    State::Preparing(ticket) | State::Draining { ticket, .. } => State::Draining {
                        ticket,
                        restart: false,
                    },
                    State::Relaunch => State::Relaunch,
                    _ => State::Resident,
                };
                let mut effects = vec![CaptureEffect::DismissOverlay];
                if invalid {
                    effects.push(CaptureEffect::ReportFailure(CaptureFailure::DisplayInvalid));
                }
                effects
            }
            CaptureAction::SetOutputBusy { busy } => {
                self.output_mutex = busy;
                vec![]
            }
        }
    }

    fn finish(&mut self, ticket: CaptureTicket, point: Option<[f64; 2]>) -> Vec<CaptureEffect> {
        if self.output_mutex {
            return vec![]; // Editing, duplicate Finish and save are disabled.
        }
        let job = {
            let State::Frozen {
                ticket: current,
                document,
            } = &self.state
            else {
                return vec![];
            };
            if *current != ticket {
                return vec![];
            }
            if point.is_some_and(|point| {
                !document
                    .selection()
                    .is_some_and(|selection| selection.contains(point))
            }) {
                return vec![]; // Double-click outside the selection is not Finish.
            }
            let Ok(crop) = document.crop() else {
                return vec![];
            };
            OutputJob {
                ticket,
                version: document.version(),
                crop,
            }
        };
        self.output_mutex = true;
        vec![CaptureEffect::ComposeOutput(job)]
    }

    fn begin(&mut self) -> Vec<CaptureEffect> {
        let Some(epoch) = self.epoch.checked_add(1) else {
            self.state = State::Relaunch;
            return vec![CaptureEffect::ReportFailure(
                CaptureFailure::NativeUncertain,
            )];
        };
        self.epoch = epoch;
        let ticket = CaptureTicket(epoch);
        self.state = State::Checking(ticket);
        vec![CaptureEffect::CheckPermission(ticket)]
    }

    fn complete(
        &mut self,
        ticket: CaptureTicket,
        result: Result<CapturedFrame, CaptureFailure>,
    ) -> Vec<CaptureEffect> {
        let restart = match self.state {
            State::Preparing(current) if current == ticket => None,
            State::Draining {
                ticket: current,
                restart,
            } if current == ticket => Some(restart),
            _ => return vec![], // Late results never open windows or overwrite another attempt.
        };
        if let Err(error) = &result
            && error.requires_relaunch()
        {
            self.state = State::Relaunch;
            return vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ReportFailure(*error),
            ];
        }
        self.state = State::Resident;
        if let Some(restart) = restart {
            return if restart { self.begin() } else { vec![] };
        }
        match result {
            Ok(frame) => {
                self.state = State::Frozen {
                    ticket,
                    document: Document::new(frame),
                };
                vec![CaptureEffect::PresentFrozen(ticket)]
            }
            Err(error) => vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ReportFailure(error),
            ],
        }
    }
}

/// One fence definition shared by publish and completion: ticket, version and crop.
fn job_matches(ticket: CaptureTicket, document: &Document, job: OutputJob) -> bool {
    ticket == job.ticket
        && document.version() == job.version
        && document.crop().is_ok_and(|crop| crop == job.crop)
}
