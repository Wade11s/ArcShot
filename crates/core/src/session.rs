//! Authority for capture preparation, cancellation and native uncertainty.
use crate::{
    Annotation, ArrowOption, BlurOption, CapturedFrame, Density, Document, LineOption, PixelRect,
    RectangleOption, Selection, TextOption, Tool,
    scroll::{
        ScrollFrameError, ScrollInputError, ScrollPermissions, ScrollPhase, ScrollPrepOutcome,
        ScrollPreparation, ScrollRefusal, ScrollTarget, ScrollTargetError, ScrollWindowError,
        scroll_frame_geometry,
    },
};
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
    /// Starts the pointer drag the active tool performs at a point.
    PointerPressed {
        /// Attempt whose overlay produced the press.
        ticket: CaptureTicket,
        /// Press point in display-local logical points.
        point: [f64; 2],
    },
    /// Updates the live preview of the active pointer drag.
    PointerMoved {
        /// Attempt whose overlay produced the move.
        ticket: CaptureTicket,
        /// Current pointer position in display-local logical points.
        point: [f64; 2],
    },
    /// Ends the active drag; changed state enters history as exactly one step.
    PointerReleased {
        /// Attempt whose overlay produced the release.
        ticket: CaptureTicket,
    },
    /// Switches the active pointer tool; interaction state, never history.
    SelectTool {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Tool the user selected.
        tool: Tool,
    },
    /// Applies one rectangle option to the tool memory and any selected object.
    RectangleOption {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Style option the user selected.
        option: RectangleOption,
    },
    /// Applies one line option to the tool memory and any selected line.
    LineOption {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Style option the user selected.
        option: LineOption,
    },
    /// Applies one arrow option to the tool memory and any selected arrow.
    ArrowOption {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Style option the user selected.
        option: ArrowOption,
    },
    /// Applies one blur option to the tool memory and any selected blur.
    BlurOption {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Style option the user selected.
        option: BlurOption,
    },
    /// Applies one text option to the tool memory, the in-progress text edit,
    /// or any selected text object.
    TextOption {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Style option the user selected.
        option: TextOption,
    },
    /// Starts a text edit at a point: a new object with the text tool, or a
    /// re-edit of the topmost text object under the select tool.
    TextBegin {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Press point in display-local logical points.
        point: [f64; 2],
    },
    /// Updates the live draft content and its measured bounds.
    TextEdited {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
        /// Current draft content, including unconfirmed IME text.
        content: String,
        /// Measured logical bounds of the draft content.
        bounds: Selection,
    },
    /// Commits the in-progress text edit as one history step.
    TextCommit {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
    },
    /// Discards the in-progress text edit without recording anything.
    TextCancel {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
    },
    /// Deletes the single selected object as one undoable step.
    DeleteSelected {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
    },
    /// Clears the single object selection without touching content.
    DeselectSelected {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
    },
    /// Undoes the most recent committed document edit.
    Undo {
        /// Attempt whose overlay produced the shortcut.
        ticket: CaptureTicket,
    },
    /// Reapplies the most recently undone document edit.
    Redo {
        /// Attempt whose overlay produced the shortcut.
        ticket: CaptureTicket,
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
    /// Opens the native PNG save panel for the current selection.
    ///
    /// Like `Finish`, this runs the shared pre-output commit/snapshot hook and
    /// takes the output mutex, so editing, repeated finish and repeated save are
    /// disabled while the panel is open. The user's destination stays in the app.
    Save {
        /// Attempt whose overlay produced the action.
        ticket: CaptureTicket,
    },
    /// Reports how the native save panel ended for exactly one fenced job.
    SavePanelResolved {
        /// Snapshot the panel was opened for.
        job: OutputJob,
        /// Chosen, cancelled or failed; a cancel is not an error.
        result: SavePanelResult,
    },
    /// Reports the result of one fenced composition/encode/clipboard job.
    OutputCompleted {
        /// Exact document version and crop the job was composed from.
        job: OutputJob,
        /// `Ok` only after the clipboard reported success.
        result: Result<(), OutputFailure>,
    },
    /// Enters Scroll preparation from the current frozen selection (I-11 step 1).
    ///
    /// The selection is reused verbatim: no re-framing, no new capture and no
    /// automatic scroll-to-top. A missing selection or existing annotations
    /// refuses without output.
    EnterScroll {
        /// Attempt whose frozen overlay produced the entry.
        ticket: CaptureTicket,
    },
    /// Reports the real three-permission Scroll preflight.
    ScrollPermissionsChecked {
        /// Preparation that requested the check.
        ticket: CaptureTicket,
        /// Actual results, never the user's intended setting.
        available: ScrollPermissions,
    },
    /// Reports the identified target window, or a classified identification failure.
    ScrollTargetIdentified {
        /// Preparation that requested the target.
        ticket: CaptureTicket,
        /// Locked public identity, or a known-state failure.
        result: Result<ScrollTarget, ScrollTargetError>,
    },
    /// Reports the fixed own-window exclusion set, or a mapping failure.
    ScrollWindowsPrepared {
        /// Preparation that requested the exclusion set.
        ticket: CaptureTicket,
        /// Mapped public window IDs, or a known-state failure.
        result: Result<Vec<u32>, ScrollWindowError>,
    },
    /// Reports the input-protection installation result (I-12 input handshake).
    ScrollInputInstalled {
        /// Preparation that requested input protection.
        ticket: CaptureTicket,
        /// The tap exists (`Ok`) or a known-state reason it does not.
        result: Result<(), ScrollInputError>,
    },
    /// Reports the trusted-initial-frame capture (I-11 step 3 / I-12 admission).
    ScrollFrameAdmitted {
        /// Preparation that requested the frame.
        ticket: CaptureTicket,
        /// An admitted named-sRGB frame, or a classified capture failure.
        result: Result<CapturedFrame, CaptureFailure>,
    },
    /// The session event tap was disabled; input protection is explicitly lost.
    ScrollInputLost {
        /// Preparation whose protection was lost.
        ticket: CaptureTicket,
    },
    /// The locked target moved, disappeared, resized or lost the foreground.
    ScrollTargetLost {
        /// Preparation whose target was lost.
        ticket: CaptureTicket,
        /// The classified target failure.
        error: ScrollTargetError,
    },
    /// Enter or an ArcShot-owned double-click completes from the single frame.
    ///
    /// Valid only while collecting: the completion document is the trusted
    /// initial frame and the one side effect is the shared clipboard Finish.
    /// This only means the user stopped collecting; it never claims coverage.
    CompleteScroll {
        /// Preparation that owns collection.
        ticket: CaptureTicket,
    },
    /// Cancels Scroll preparation and keeps the region selection (I-11 step 10).
    CancelScroll {
        /// Preparation to cancel.
        ticket: CaptureTicket,
    },
    /// A Scroll native request's state could not be confirmed (I-12).
    ///
    /// This is terminal: no output, no partial result and no in-session restart.
    ScrollNativeUncertain {
        /// Preparation whose native state is uncertain.
        ticket: CaptureTicket,
    },
    /// Invalidates the built-in screen; never migrates to another display.
    DisplayInvalidated,
}

/// Tasks to execute outside the pure core, guarded by their attempt ticket.
#[derive(Clone, Debug, PartialEq)]
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
    /// Show the native PNG save panel for a fenced explicit-save snapshot.
    RequestSavePanel(OutputJob),
    /// Play the short completion sound after successful clipboard or file output.
    PlayCompletionSound,
    /// Report an output failure without discarding the document.
    ReportOutputFailure(OutputFailure),
    /// Preflight the three Scroll-only permissions.
    CheckScrollPermissions(CaptureTicket),
    /// Identify and lock the single target window for the frozen selection.
    IdentifyScrollTarget(CaptureTicket),
    /// Fix ArcShot's own visible window set and map it to one exclusion array.
    PrepareScrollWindows(CaptureTicket),
    /// Native Scroll preparation finished; the app installs input protection next.
    ScrollPrepared(CaptureTicket),
    /// Input protection was established; capture the one trusted initial frame.
    CaptureScrollFrame(CaptureTicket),
    /// Collection opened; the control UI may release raw scroll in the selection.
    ScrollCollecting(CaptureTicket),
    /// Preparation stopped with a known-state reason; keep the region session.
    ScrollRefused(CaptureTicket, ScrollRefusal),
    /// Native state is uncertain; close Scroll UI and require a relaunch.
    ScrollRelaunchRequired,
}

/// Where one fenced output action publishes its encoded PNG.
///
/// Finish and explicit Save share the compose/encode result for a document
/// version but never trigger each other's side effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputTarget {
    /// Finish: copy to the pasteboard, never write a file.
    Clipboard,
    /// Explicit Save: write the chosen file, never copy to the pasteboard.
    SaveFile,
}

/// How the native PNG save panel ended for one fenced output job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavePanelResult {
    /// The user chose a destination; compose and encode for the file write.
    Chosen,
    /// The user cancelled; the untouched document stays editable and this is
    /// not an error.
    Cancelled,
    /// The panel could not be shown or yielded no local file URL.
    Failed,
}

/// One version-fenced output job: the document may not change before publishing.
///
/// The annotation snapshot belongs to the fenced version so the worker never
/// reads mutable document state and preview/output share one rasterizer.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputJob {
    ticket: CaptureTicket,
    version: u64,
    crop: PixelRect,
    target: OutputTarget,
    annotations: Vec<Annotation>,
}
impl OutputJob {
    /// Overlay attempt that owns the document.
    #[must_use]
    pub const fn ticket(&self) -> CaptureTicket {
        self.ticket
    }
    /// Document version the job was composed from.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    /// Physical crop inside the immutable base image.
    #[must_use]
    pub const fn crop(&self) -> PixelRect {
        self.crop
    }
    /// Layer-ordered annotation snapshot this job was composed from.
    #[must_use]
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }
    /// The one side effect this job may perform after encoding.
    #[must_use]
    pub const fn target(&self) -> OutputTarget {
        self.target
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
    /// The native save panel could not be shown or produced no file URL.
    SavePanel,
    /// The chosen PNG file was not confirmed written.
    SaveFile,
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
            Self::SavePanel => "无法打开 PNG 保存对话框，未产生输出；选区已保留，可重试",
            Self::SaveFile => "PNG 未确认写入所选文件，未报告成功；选区已保留，可重试",
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
        /// Boxed so the session state enum stays small next to the edit history.
        document: Box<Document>,
    },
    Relaunch,
}

/// The one fenced output operation that owns the shared output/save mutex.
#[derive(Clone, Debug, PartialEq)]
enum OutputStage {
    /// A native save panel is open; no encoded bytes exist yet.
    Panel(OutputJob),
    /// The fixed snapshot is being composed/encoded before one side effect.
    Encoding(OutputJob),
}

/// The sole capture authority; contains no system handles, clocks or I/O.
#[derive(Debug)]
pub struct CaptureSession {
    epoch: u64,
    state: State,
    output: Option<OutputStage>,
    /// Persisted menu preference; off makes a successful output silent.
    completion_sound: bool,
    /// I-09 option density. Session-scoped (outside the replaceable `Document`),
    /// so switching it never clears the tool, styles, selection or objects.
    density: Density,
    /// I-11/I-12 Scroll preparation entered from the frozen selection. It is a
    /// sub-flow of the one Capture Session: it reuses the selection and never
    /// starts a second capture. `None` while Region Capture is active.
    scroll: Option<ScrollPreparation>,
}
impl Default for CaptureSession {
    fn default() -> Self {
        Self {
            epoch: 0,
            state: State::Resident,
            output: None,
            // I-04: the completion sound is on unless the user turns it off.
            completion_sound: true,
            // I-09: Compact is the documented default.
            density: Density::Compact,
            scroll: None,
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
        self.output.is_some()
    }

    /// Whether successful output plays the short completion sound.
    #[must_use]
    pub const fn completion_sound(&self) -> bool {
        self.completion_sound
    }

    /// Applies the menu preference; it survives session restarts by design.
    ///
    /// Silence only affects a successful output. Failures are always reported.
    pub const fn set_completion_sound(&mut self, enabled: bool) {
        self.completion_sound = enabled;
    }

    /// The current option density; it survives new captures and cancellations.
    #[must_use]
    pub const fn density(&self) -> Density {
        self.density
    }

    /// Applies the option density without touching the authoritative document.
    ///
    /// This is a view preference, not a semantic edit: no [`CaptureEffect`] is
    /// produced and the frozen document is left byte-for-byte unchanged.
    pub const fn set_density(&mut self, density: Density) {
        self.density = density;
    }

    /// The current Scroll preparation phase; `Inactive` when Region Capture is active.
    #[must_use]
    pub const fn scroll_phase(&self) -> ScrollPhase {
        match &self.scroll {
            Some(preparation) => preparation.phase(),
            None => ScrollPhase::Inactive,
        }
    }

    /// The single selection Scroll preparation reuses, if one is active.
    #[must_use]
    pub const fn scroll_selection(&self) -> Option<Selection> {
        match &self.scroll {
            Some(preparation) => Some(preparation.selection()),
            None => None,
        }
    }

    /// The locked public target identity, once identification succeeded.
    #[must_use]
    pub fn scroll_target(&self) -> Option<&ScrollTarget> {
        self.scroll.as_ref().and_then(ScrollPreparation::target)
    }

    /// The fixed own-window exclusion set, once preparation finished.
    #[must_use]
    pub fn scroll_exclusion(&self) -> Option<&[u32]> {
        self.scroll.as_ref().and_then(ScrollPreparation::exclusion)
    }

    /// The refusal that stopped Scroll preparation, if one is exposed.
    #[must_use]
    pub fn scroll_refusal(&self) -> Option<&ScrollRefusal> {
        self.scroll.as_ref().and_then(ScrollPreparation::refusal)
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
            State::Frozen { document, .. } => Some(document.as_ref()),
            _ => None,
        }
    }

    /// Fences publishing encoded output immediately before either side effect.
    ///
    /// The job must be the exact snapshot the session is encoding, still belong
    /// to the current attempt, and match the current document version and crop.
    /// A save panel that is still open has no encoded bytes and publishes nothing.
    #[must_use]
    pub fn may_publish(&self, job: &OutputJob) -> bool {
        let State::Frozen { ticket, document } = &self.state else {
            return false;
        };
        matches!(&self.output, Some(OutputStage::Encoding(active)) if active == job)
            && job_matches(*ticket, document, job)
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

    /// Borrows the frozen document for an edit, ignoring output-busy and stale tickets.
    fn editable(&mut self, ticket: CaptureTicket) -> Option<&mut Document> {
        if self.output.is_some() {
            return None;
        }
        match &mut self.state {
            State::Frozen {
                ticket: current,
                document,
            } if *current == ticket => Some(document),
            _ => None,
        }
    }

    /// Applies a semantic action and returns explicit effects for the app coordinator.
    pub fn dispatch(&mut self, action: CaptureAction) -> Vec<CaptureEffect> {
        match action {
            // The shared output/save mutex ignores the capture hotkey and the
            // menu Start alike; output slices release it explicitly.
            CaptureAction::Start if self.output.is_some() => vec![],
            CaptureAction::Start => {
                // A hotkey restart abandons any Scroll preparation together with
                // the region session; the new capture starts from Resident.
                self.scroll = None;
                match self.state {
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
                }
            }
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
                if *current != ticket || self.output.is_some() {
                    return vec![];
                }
                let Ok(selection) = Selection::from_drag(from, to) else {
                    return vec![];
                };
                // An out-of-bounds or over-budget drag leaves the prior document.
                let _ = document.select(selection);
                vec![]
            }
            CaptureAction::PointerPressed { ticket, point } => {
                if let Some(document) = self.editable(ticket) {
                    document.begin_pointer(point);
                }
                vec![]
            }
            CaptureAction::PointerMoved { ticket, point } => {
                if let Some(document) = self.editable(ticket) {
                    document.update_drag(point);
                }
                vec![]
            }
            CaptureAction::PointerReleased { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.end_drag();
                }
                vec![]
            }
            CaptureAction::SelectTool { ticket, tool } => {
                if let Some(document) = self.editable(ticket) {
                    document.set_tool(tool);
                }
                vec![]
            }
            CaptureAction::RectangleOption { ticket, option } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.apply_rectangle_option(option);
                }
                vec![]
            }
            CaptureAction::LineOption { ticket, option } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.apply_line_option(option);
                }
                vec![]
            }
            CaptureAction::ArrowOption { ticket, option } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.apply_arrow_option(option);
                }
                vec![]
            }
            CaptureAction::BlurOption { ticket, option } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.apply_blur_option(option);
                }
                vec![]
            }
            CaptureAction::TextOption { ticket, option } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.apply_text_option(option);
                }
                vec![]
            }
            CaptureAction::TextBegin { ticket, point } => {
                if let Some(document) = self.editable(ticket) {
                    document.begin_text_edit(point);
                }
                vec![]
            }
            CaptureAction::TextEdited {
                ticket,
                content,
                bounds,
            } => {
                if let Some(document) = self.editable(ticket) {
                    document.update_text_edit(content, bounds);
                }
                vec![]
            }
            CaptureAction::TextCommit { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.commit_edits();
                }
                vec![]
            }
            CaptureAction::TextCancel { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    document.cancel_text_edit();
                }
                vec![]
            }
            CaptureAction::DeleteSelected { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.delete_selected();
                }
                vec![]
            }
            CaptureAction::DeselectSelected { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    document.deselect();
                }
                vec![]
            }
            CaptureAction::Undo { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.undo();
                }
                vec![]
            }
            CaptureAction::Redo { ticket } => {
                if let Some(document) = self.editable(ticket) {
                    let _ = document.redo();
                }
                vec![]
            }
            CaptureAction::Finish { ticket } => {
                self.begin_output(ticket, OutputTarget::Clipboard, None)
            }
            CaptureAction::FinishAt { ticket, point } => {
                self.begin_output(ticket, OutputTarget::Clipboard, Some(point))
            }
            CaptureAction::Save { ticket } => {
                self.begin_output(ticket, OutputTarget::SaveFile, None)
            }
            CaptureAction::SavePanelResolved { job, result } => self.resolve_panel(job, result),
            CaptureAction::OutputCompleted { job, result } => {
                let matches_job = match &self.state {
                    State::Frozen { ticket, document } => job_matches(*ticket, document, &job),
                    _ => false,
                };
                let publishing =
                    matches!(&self.output, Some(OutputStage::Encoding(active)) if active == &job);
                // Stale or duplicated results never publish or end another session.
                if !matches_job || !publishing {
                    return vec![];
                }
                self.output = None;
                match result {
                    Ok(()) => {
                        self.state = State::Resident;
                        // A successful Scroll completion ends the whole session;
                        // the stashed region document is not restored.
                        self.scroll = None;
                        let mut effects = vec![CaptureEffect::DismissOverlay];
                        // The preference only silences success; failure still reports.
                        if self.completion_sound {
                            effects.push(CaptureEffect::PlayCompletionSound);
                        }
                        effects
                    }
                    Err(error) => {
                        // A failed Scroll completion keeps the collected result:
                        // the region document returns so Esc and retry work.
                        self.restore_scroll_region_document();
                        vec![CaptureEffect::ReportOutputFailure(error)]
                    }
                }
            }
            CaptureAction::EnterScroll { ticket } => {
                if self.output.is_some() {
                    return vec![];
                }
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
                let Some(selection) = document.selection() else {
                    return vec![CaptureEffect::ScrollRefused(
                        ticket,
                        ScrollRefusal::NoSelection,
                    )];
                };
                if !document.annotations().is_empty() {
                    return vec![CaptureEffect::ScrollRefused(
                        ticket,
                        ScrollRefusal::AnnotationsPresent,
                    )];
                }
                self.scroll = Some(ScrollPreparation::new(ticket, selection));
                vec![
                    CaptureEffect::DismissOverlay,
                    CaptureEffect::CheckScrollPermissions(ticket),
                ]
            }
            CaptureAction::ScrollPermissionsChecked { ticket, available } => {
                let Some(preparation) = self.scroll.as_mut() else {
                    return vec![];
                };
                if preparation.ticket() != ticket {
                    return vec![];
                }
                let outcome = preparation.check_permissions(available);
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::ScrollTargetIdentified { ticket, result } => {
                let Some(preparation) = self.scroll.as_mut() else {
                    return vec![];
                };
                if preparation.ticket() != ticket {
                    return vec![];
                }
                let outcome = preparation.lock_target(result);
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::ScrollWindowsPrepared { ticket, result } => {
                let Some(preparation) = self.scroll.as_mut() else {
                    return vec![];
                };
                if preparation.ticket() != ticket {
                    return vec![];
                }
                let outcome = preparation.fix_windows(result);
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::ScrollInputInstalled { ticket, result } => {
                let Some(preparation) = self.scroll.as_mut() else {
                    return vec![];
                };
                if preparation.ticket() != ticket {
                    return vec![];
                }
                let outcome = preparation.install_input(result);
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::ScrollFrameAdmitted { ticket, result } => {
                self.admit_scroll_frame(ticket, result)
            }
            CaptureAction::ScrollInputLost { ticket } => {
                // An output already in flight outranks protection: the frame is
                // trusted, so the publish fence governs and this is too late.
                if self.output.is_some() {
                    return vec![];
                }
                let Some(preparation) = self.scroll.as_mut() else {
                    return vec![];
                };
                if preparation.ticket() != ticket {
                    return vec![];
                }
                let outcome = preparation.lose_input_protection();
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::ScrollTargetLost { ticket, error } => {
                // Same fence as input loss: a running output completes anyway.
                if self.output.is_some() {
                    return vec![];
                }
                let Some(preparation) = self.scroll.as_mut() else {
                    return vec![];
                };
                if preparation.ticket() != ticket {
                    return vec![];
                }
                let outcome = preparation.stop_collecting(ScrollRefusal::Target(error));
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::CompleteScroll { ticket } => self.complete_scroll(ticket),
            CaptureAction::CancelScroll { ticket } => {
                let owned = self
                    .scroll
                    .as_ref()
                    .is_some_and(|preparation| preparation.ticket() == ticket);
                if !owned {
                    return vec![];
                }
                // A completion output in flight is abandoned with its fence.
                self.output = None;
                let outcome = self
                    .scroll
                    .as_mut()
                    .map_or(ScrollPrepOutcome::Ignored, ScrollPreparation::stop);
                self.finish_scroll_outcome(ticket, outcome)
            }
            CaptureAction::ScrollNativeUncertain { ticket } => {
                if !self
                    .scroll
                    .as_ref()
                    .is_some_and(|preparation| preparation.ticket() == ticket)
                {
                    return vec![];
                }
                self.scroll = None;
                self.output = None;
                self.state = State::Relaunch;
                vec![
                    CaptureEffect::DismissOverlay,
                    CaptureEffect::ScrollRelaunchRequired,
                ]
            }
            CaptureAction::Cancel | CaptureAction::DisplayInvalidated => {
                // A cancelled output job may still finish; its fence must fail.
                self.output = None;
                // Display loss and cancellation abandon Scroll preparation too;
                // the region session is kept for a known Scroll refusal only.
                self.scroll = None;
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

    /// Maps one pure Scroll preparation transition to app effects.
    ///
    /// A known-state refusal or a user cancel clears the preparation and
    /// re-presents the frozen region overlay, so the selection is preserved and
    /// no output is produced.
    fn finish_scroll_outcome(
        &mut self,
        ticket: CaptureTicket,
        outcome: ScrollPrepOutcome,
    ) -> Vec<CaptureEffect> {
        match outcome {
            ScrollPrepOutcome::CheckPermissions => {
                vec![CaptureEffect::CheckScrollPermissions(ticket)]
            }
            ScrollPrepOutcome::IdentifyTarget => vec![CaptureEffect::IdentifyScrollTarget(ticket)],
            ScrollPrepOutcome::PrepareWindows => vec![CaptureEffect::PrepareScrollWindows(ticket)],
            ScrollPrepOutcome::Prepared => vec![CaptureEffect::ScrollPrepared(ticket)],
            ScrollPrepOutcome::CaptureFrame => vec![CaptureEffect::CaptureScrollFrame(ticket)],
            ScrollPrepOutcome::Collecting => vec![CaptureEffect::ScrollCollecting(ticket)],
            ScrollPrepOutcome::Ignored => vec![],
            ScrollPrepOutcome::Refused(refusal) => {
                // A refusal never abandons output the user already owns: the
                // region document returns before the overlay re-presents.
                self.restore_scroll_region_document();
                self.scroll = None;
                vec![
                    CaptureEffect::ScrollRefused(ticket, refusal),
                    CaptureEffect::PresentFrozen(ticket),
                ]
            }
            ScrollPrepOutcome::Stopped => {
                self.restore_scroll_region_document();
                self.scroll = None;
                vec![CaptureEffect::PresentFrozen(ticket)]
            }
        }
    }

    /// Returns a stashed region document to the frozen state, if one exists.
    ///
    /// Collection stashes the region document while a completion output runs;
    /// abandon and failure paths put it back so Esc always returns the
    /// pre-scroll selection on the same frozen frame.
    fn restore_scroll_region_document(&mut self) {
        let Some(region) = self
            .scroll
            .as_mut()
            .and_then(ScrollPreparation::take_region_document)
        else {
            return;
        };
        if let State::Frozen { document, .. } = &mut self.state {
            *document = region;
        }
    }

    /// Validates and admits one Scroll frame result (I-12 admission contract).
    ///
    /// Uncertain native state and display loss reuse the terminal paths; every
    /// known-state frame failure refuses without output and keeps the region
    /// session. An admitted frame must exactly match the fixed display,
    /// selection and scale through the shared `scroll_frame_geometry` conversion.
    fn admit_scroll_frame(
        &mut self,
        ticket: CaptureTicket,
        result: Result<CapturedFrame, CaptureFailure>,
    ) -> Vec<CaptureEffect> {
        let matches_prepared = self.scroll.as_ref().is_some_and(|preparation| {
            preparation.ticket() == ticket && preparation.phase() == ScrollPhase::Prepared
        });
        if !matches_prepared {
            return vec![]; // A late or duplicate frame never opens collection.
        }
        let failure = match result {
            Ok(frame) => {
                let expected = self.scroll_frame_expectation();
                if expected.is_some_and(|geometry| frame.geometry() == geometry) {
                    let outcome = self
                        .scroll
                        .as_mut()
                        .map_or(ScrollPrepOutcome::Ignored, |preparation| {
                            preparation.admit_frame(frame)
                        });
                    return self.finish_scroll_outcome(ticket, outcome);
                }
                ScrollFrameError::Geometry
            }
            Err(CaptureFailure::InvalidFrame) => ScrollFrameError::Admission,
            Err(CaptureFailure::ResourceLimit) => ScrollFrameError::ResourceLimit,
            Err(CaptureFailure::WorkerUnavailable) => ScrollFrameError::Worker,
            Err(CaptureFailure::DisplayInvalid) => {
                return self.dispatch(CaptureAction::DisplayInvalidated);
            }
            Err(CaptureFailure::NativeUncertain) | Err(CaptureFailure::WorkerPanicked) => {
                return self.dispatch(CaptureAction::ScrollNativeUncertain { ticket });
            }
        };
        let outcome = self
            .scroll
            .as_mut()
            .map_or(ScrollPrepOutcome::Ignored, |preparation| {
                preparation.stop_prepared(ScrollRefusal::Frame(failure))
            });
        self.finish_scroll_outcome(ticket, outcome)
    }

    /// The frame geometry the fixed selection must produce, if it is still valid.
    fn scroll_frame_expectation(&self) -> Option<crate::ScreenGeometry> {
        let State::Frozen { document, .. } = &self.state else {
            return None;
        };
        let preparation = self.scroll.as_ref()?;
        let region = document.frame().geometry();
        let crop = preparation.selection().pixel_rect(region).ok()?;
        scroll_frame_geometry(region, crop).ok()
    }

    /// Completes from the trusted single frame (I-11 step 6).
    ///
    /// The completion document is the admitted frame with a whole-frame
    /// selection, run through the same fenced clipboard Finish as a region
    /// capture. The region document is stashed so Esc and failure return the
    /// pre-scroll selection; success ends the session without restoring it.
    /// Completing only means the user stopped collecting: it never claims
    /// top/bottom coverage.
    fn complete_scroll(&mut self, ticket: CaptureTicket) -> Vec<CaptureEffect> {
        if self.output.is_some() {
            return vec![]; // Repeated completion is disabled while output runs.
        }
        let may_complete = self.scroll.as_ref().is_some_and(|preparation| {
            preparation.ticket() == ticket
                && preparation.phase() == ScrollPhase::Collecting
                && preparation.frame().is_some()
        }) && matches!(self.state, State::Frozen { ticket: current, .. } if current == ticket);
        if !may_complete {
            return vec![];
        }
        let frame = self
            .scroll
            .as_ref()
            .and_then(ScrollPreparation::frame)
            .cloned();
        let Some(frame) = frame else {
            return vec![];
        };
        let geometry = frame.geometry();
        let [width, height] = geometry.pixel_size();
        let scale = geometry.scale();
        let Some(full) = Selection::try_new(
            0.0,
            0.0,
            f64::from(width) / scale,
            f64::from(height) / scale,
        )
        .ok() else {
            return vec![];
        };
        let mut completion = Document::new(frame);
        if completion.select(full).is_err() {
            return vec![]; // A completion document must publish its whole frame.
        }
        let State::Frozen { document, .. } = &mut self.state else {
            return vec![];
        };
        let region = std::mem::replace(document, Box::new(completion));
        if let Some(preparation) = self.scroll.as_mut() {
            preparation.stash_region_document(region);
        }
        self.begin_output(ticket, OutputTarget::Clipboard, None)
    }

    /// Runs the shared pre-output contract, then fences one output snapshot.
    ///
    /// Finish and Save both pass through here: pending edits are committed first
    /// (no text tool in this slice), then ticket, selection and the pre-output
    /// snapshot (document version and physical crop) are fixed before any side
    /// effect. The target only decides which single side effect may follow.
    fn begin_output(
        &mut self,
        ticket: CaptureTicket,
        target: OutputTarget,
        point: Option<[f64; 2]>,
    ) -> Vec<CaptureEffect> {
        if self.output.is_some() {
            return vec![]; // Editing, duplicate Finish and repeated Save stay disabled.
        }
        let job = {
            let State::Frozen {
                ticket: current,
                document,
            } = &mut self.state
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
            if document.commit_edits().is_err() {
                return vec![]; // A failed commit must not publish a partial snapshot.
            }
            let Ok(crop) = document.crop() else {
                return vec![];
            };
            OutputJob {
                ticket,
                version: document.version(),
                crop,
                target,
                annotations: document.annotations().to_vec(),
            }
        };
        match target {
            OutputTarget::Clipboard => {
                self.output = Some(OutputStage::Encoding(job.clone()));
                vec![CaptureEffect::ComposeOutput(job)]
            }
            OutputTarget::SaveFile => {
                self.output = Some(OutputStage::Panel(job.clone()));
                vec![CaptureEffect::RequestSavePanel(job)]
            }
        }
    }

    /// Applies one native save-panel outcome under the same snapshot fence.
    fn resolve_panel(&mut self, job: OutputJob, result: SavePanelResult) -> Vec<CaptureEffect> {
        if !matches!(&self.output, Some(OutputStage::Panel(active)) if active == &job) {
            return vec![]; // Closed, stale or duplicated panel results change nothing.
        }
        let matches_document = match &self.state {
            State::Frozen { ticket, document } => job_matches(*ticket, document, &job),
            _ => false,
        };
        if !matches_document {
            self.output = None;
            return vec![];
        }
        match result {
            SavePanelResult::Chosen => {
                self.output = Some(OutputStage::Encoding(job.clone()));
                vec![CaptureEffect::ComposeOutput(job)]
            }
            SavePanelResult::Cancelled => {
                self.output = None;
                vec![]
            }
            SavePanelResult::Failed => {
                self.output = None;
                vec![CaptureEffect::ReportOutputFailure(OutputFailure::SavePanel)]
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
                self.state = State::Frozen {
                    ticket,
                    document: Box::new(Document::new(frame)),
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
fn job_matches(ticket: CaptureTicket, document: &Document, job: &OutputJob) -> bool {
    ticket == job.ticket
        && document.version() == job.version
        && document.crop().is_ok_and(|crop| crop == job.crop)
}
