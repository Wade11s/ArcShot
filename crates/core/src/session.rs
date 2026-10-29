//! Authority for capture preparation, cancellation and native uncertainty.
use crate::CapturedFrame;
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
    /// Invalidates the built-in screen; never migrates to another display.
    DisplayInvalidated,
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
}

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
        frame: CapturedFrame,
    },
    Relaunch,
}

/// The sole capture authority; contains no system handles, clocks or I/O.
#[derive(Debug)]
pub struct CaptureSession {
    epoch: u64,
    state: State,
}
impl Default for CaptureSession {
    fn default() -> Self {
        Self {
            epoch: 0,
            state: State::Resident,
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

    /// Borrows the accepted immutable frame; cancelled/terminal sessions expose none.
    #[must_use]
    pub fn frame(&self) -> Option<&CapturedFrame> {
        match &self.state {
            State::Frozen { frame, .. } => Some(frame),
            _ => None,
        }
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
            CaptureAction::Cancel | CaptureAction::DisplayInvalidated => {
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
        }
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
                self.state = State::Frozen { ticket, frame };
                vec![CaptureEffect::PresentFrozen(ticket)]
            }
            Err(error) => vec![
                CaptureEffect::DismissOverlay,
                CaptureEffect::ReportFailure(error),
            ],
        }
    }
}
