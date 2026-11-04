//! Pure Scroll Capture preparation and the protected single-frame collection
//! loop: permissions, target lock, own-window exclusion, input handshake and
//! trusted first frame.
//!
//! This module is the authoritative, platform-neutral half of I-11 steps 1–3
//! and 6/10 plus the permission/target/input half of I-12. It never touches
//! system I/O: the app injects the real permission results, the identified
//! target, the fixed exclusion set, the input-protection result and the
//! admitted frame; this module decides whether preparation may continue,
//! collection may open, completion may publish, or the process must relaunch.
use crate::{
    CaptureTicket, CapturedFrame, Document, PixelRect, ScreenGeometry, Selection, frame::FrameError,
};

/// The three permissions Scroll Capture requires, in preflight order.
///
/// Region Capture and its hotkey only require [`Self::ScreenRecording`]; the
/// other two are Scroll-only gates (I-04).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScrollPermission {
    /// Freeze the screen for capture.
    ScreenRecording,
    /// Identify the pointer target and protect the scroll area from stray clicks.
    InputMonitoring,
    /// Confirm the public target window identity.
    Accessibility,
}

impl ScrollPermission {
    /// Preflight order shared by the native adapter and the notice.
    pub const ALL: [Self; 3] = [
        Self::ScreenRecording,
        Self::InputMonitoring,
        Self::Accessibility,
    ];

    /// Stable key used by diagnostics and AX identifiers.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ScreenRecording => "screen_recording",
            Self::InputMonitoring => "input_monitoring",
            Self::Accessibility => "accessibility",
        }
    }

    /// User-facing purpose explanation; never a speculation about TCC state.
    #[must_use]
    pub const fn purpose(self) -> &'static str {
        match self {
            Self::ScreenRecording => "捕获屏幕画面，用于滚动截图。",
            Self::InputMonitoring => "识别指针下的目标窗口并拦截误触点击，不记录按键内容。",
            Self::Accessibility => "确认目标窗口的公开身份与几何，不读取窗口内容。",
        }
    }

    /// User-facing name for the permission.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ScreenRecording => "屏幕录制",
            Self::InputMonitoring => "输入监控",
            Self::Accessibility => "辅助功能",
        }
    }
}

/// Actual preflight results for the three Scroll permissions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScrollPermissions {
    /// Screen Recording is available.
    pub screen_recording: bool,
    /// Input Monitoring is available.
    pub input_monitoring: bool,
    /// Accessibility is available.
    pub accessibility: bool,
}

impl ScrollPermissions {
    /// Whether one permission is available.
    #[must_use]
    pub const fn available(self, permission: ScrollPermission) -> bool {
        match permission {
            ScrollPermission::ScreenRecording => self.screen_recording,
            ScrollPermission::InputMonitoring => self.input_monitoring,
            ScrollPermission::Accessibility => self.accessibility,
        }
    }

    /// The permissions that are still missing, in preflight order.
    #[must_use]
    pub fn missing(self) -> Vec<ScrollPermission> {
        ScrollPermission::ALL
            .into_iter()
            .filter(|permission| !self.available(*permission))
            .collect()
    }

    /// Whether every Scroll permission is available.
    #[must_use]
    pub fn all_available(self) -> bool {
        ScrollPermission::ALL
            .into_iter()
            .all(|permission| self.available(permission))
    }
}

/// A target window rectangle in display-local logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollTargetFrame {
    /// Display-local left edge.
    pub x: f64,
    /// Display-local top edge.
    pub y: f64,
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
}

impl ScrollTargetFrame {
    /// Whether the rectangle has finite, positive extents.
    #[must_use]
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }

    /// Whether a display-local selection lies entirely inside this rectangle.
    #[must_use]
    pub fn contains(self, selection: Selection) -> bool {
        if !self.is_valid() {
            return false;
        }
        selection.x() >= self.x
            && selection.y() >= self.y
            && selection.x() + selection.width() <= self.x + self.width
            && selection.y() + selection.height() <= self.y + self.height
    }
}

/// The public identity of the single target window, locked at scroll entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollTarget {
    /// Public window identifier (CoreGraphics/SCWindow window ID).
    pub window_id: u32,
    /// Owning application bundle identifier; never a window title.
    pub owner: String,
    /// Owning application process identifier, for foreground checks.
    pub owner_pid: u32,
    /// Target window frame in display-local logical points.
    pub frame: ScrollTargetFrame,
}

/// Why a target window could not be locked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollTargetError {
    /// No layer-0 on-screen window contains the whole selection.
    NoTarget,
    /// More than one window contains the whole selection.
    AmbiguousTarget,
    /// The owning application is not frontmost.
    NotFrontmost,
    /// The pointer's public hit-test window differs from the locked target.
    PointerMismatch,
    /// The locked geometry cannot contain the selection.
    Geometry,
    /// The native query itself could not be confirmed.
    NativeUncertain,
}

/// Why ArcShot's own visible window set could not be fixed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollWindowError {
    /// A still-visible owned window did not map to an `SCWindow`.
    MappingFailed,
    /// The native query itself could not be confirmed.
    NativeUncertain,
    /// The query was made away from the main thread.
    NotMainThread,
}

/// Why the Scroll input protection could not be established or was lost.
///
/// Every variant is a known state: collection stops without output and the
/// region session survives for another attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScrollInputError {
    /// The target application could not be brought to the front.
    NotFrontmost,
    /// The session event tap could not be created or enabled.
    TapUnavailable,
    /// A physical button or capture key was held when protection started.
    GestureInProgress,
    /// The system disabled the tap; protection is explicitly lost.
    ProtectionLost,
    /// A previous tap is still releasing the input it owns.
    StillDraining,
}

/// Why a Scroll initial frame could not be trusted.
///
/// These are known-state content failures: collection stops without output.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScrollFrameError {
    /// The frame failed admission (dimensions, stride, bytes or opacity).
    Admission,
    /// The frame geometry does not match the fixed display/selection/scale.
    Geometry,
    /// The capture exceeds the Scroll resource budget.
    ResourceLimit,
    /// The capture worker could not start; no native request began.
    Worker,
}

/// Why one new frame could not extend the verified stitch history (I-11 §4).
///
/// Every variant is a known-state stop: collection ends without output and the
/// verified result stays available behind the explicit partial-result seam.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScrollStitchError {
    /// The frame geometry differs from the fixed selection or scale.
    Geometry,
    /// More than one vertical displacement explains the overlap exactly.
    Ambiguous,
    /// The overlap is not fully consistent with verified history.
    ContentChanged,
    /// No overlap matches: the viewport jumped a gap.
    Gap,
    /// The extension would exceed V1's document budget.
    ResourceLimit,
}

/// Why Scroll preparation or collection stopped without producing output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScrollRefusal {
    /// The frozen document has no valid selection.
    NoSelection,
    /// The selection already carries annotations; the user must undo/delete them.
    AnnotationsPresent,
    /// One or more Scroll permissions are missing.
    Permissions(Vec<ScrollPermission>),
    /// The target window could not be locked.
    Target(ScrollTargetError),
    /// The own-window exclusion set could not be fixed.
    Windows(ScrollWindowError),
    /// The input protection could not be established or was lost.
    Input(ScrollInputError),
    /// The trusted first frame could not be captured or admitted.
    Frame(ScrollFrameError),
    /// A later frame could not be verified against the stitched history.
    Stitch(ScrollStitchError),
}

/// Observable Scroll lifecycle, independent of the region phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollPhase {
    /// No Scroll preparation is active.
    Inactive,
    /// The three permissions are being checked.
    CheckingPermissions,
    /// The single target window is being identified.
    IdentifyingTarget,
    /// ArcShot's own visible window set is being fixed.
    PreparingWindows,
    /// Native preparation finished; input protection and the first frame come next.
    Prepared,
    /// Input protection is active and the trusted first frame was admitted.
    Collecting,
    /// Preparation or collection stopped; the region session and its selection are kept.
    Stopped,
}

/// The state transition one Scroll action produces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScrollPrepOutcome {
    /// Ask the app to preflight the three permissions.
    CheckPermissions,
    /// Ask the app to identify and lock the target window.
    IdentifyTarget,
    /// Ask the app to fix and map the own-window exclusion set.
    PrepareWindows,
    /// Native preparation finished.
    Prepared,
    /// Input protection was established; ask the app to capture the first frame.
    CaptureFrame,
    /// The trusted first frame was admitted; collection may open.
    Collecting,
    /// A raw collection frame was admitted; the app must verify and extend it.
    VerifyFrame,
    /// A frame was strictly verified; the verified preview may refresh.
    StitchUpdated,
    /// Preparation stopped with a known-state refusal; keep the region session.
    Refused(ScrollRefusal),
    /// Preparation was cancelled; keep the region session.
    Stopped,
    /// The action belongs to another phase or a finished attempt; nothing changes.
    Ignored,
}

/// One Scroll preparation entered from a frozen region selection.
#[derive(Debug)]
pub struct ScrollPreparation {
    ticket: CaptureTicket,
    selection: Selection,
    phase: ScrollPhase,
    permissions: ScrollPermissions,
    target: Option<ScrollTarget>,
    exclusion: Option<Vec<u32>>,
    refusal: Option<ScrollRefusal>,
    /// The one trusted initial frame; the single-frame loop never queues a second.
    frame: Option<CapturedFrame>,
    /// Latest verified stitched document. It starts as the origin frame and is
    /// replaced only after the image verifier proves a unique, contiguous,
    /// fully overlapping extension; reverse passes leave it unchanged.
    stitch: Option<CapturedFrame>,
    /// A raw frame admitted by the app, awaiting image verification.
    pending_frame: Option<CapturedFrame>,
    /// Verified extensions and de-duplicated reverse passes, for diagnostics.
    extensions: u64,
    duplicates: u64,
    /// The region document, stashed while a completion output runs; restored on
    /// abandon or failure so Esc always returns the pre-scroll selection.
    region_document: Option<Box<Document>>,
}

impl ScrollPreparation {
    /// Starts preparation for one frozen selection.
    #[must_use]
    pub fn new(ticket: CaptureTicket, selection: Selection) -> Self {
        Self {
            ticket,
            selection,
            phase: ScrollPhase::CheckingPermissions,
            permissions: ScrollPermissions {
                screen_recording: false,
                input_monitoring: false,
                accessibility: false,
            },
            target: None,
            exclusion: None,
            refusal: None,
            frame: None,
            stitch: None,
            pending_frame: None,
            extensions: 0,
            duplicates: 0,
            region_document: None,
        }
    }

    /// The region attempt that owns this preparation.
    #[must_use]
    pub const fn ticket(&self) -> CaptureTicket {
        self.ticket
    }

    /// Current preparation phase.
    #[must_use]
    pub const fn phase(&self) -> ScrollPhase {
        self.phase
    }

    /// The single selection this preparation reuses; it is never re-framed.
    #[must_use]
    pub const fn selection(&self) -> Selection {
        self.selection
    }

    /// The locked target, once identification succeeded.
    #[must_use]
    pub fn target(&self) -> Option<&ScrollTarget> {
        self.target.as_ref()
    }

    /// The fixed own-window exclusion set, once prepared.
    #[must_use]
    pub fn exclusion(&self) -> Option<&[u32]> {
        self.exclusion.as_deref()
    }

    /// The refusal that stopped preparation, if any.
    #[must_use]
    pub fn refusal(&self) -> Option<&ScrollRefusal> {
        self.refusal.as_ref()
    }

    /// The trusted initial frame, once collection has opened.
    #[must_use]
    pub fn frame(&self) -> Option<&CapturedFrame> {
        self.frame.as_ref()
    }

    /// The latest verified stitched document; the origin frame until a later
    /// frame is strictly verified.
    #[must_use]
    pub fn stitch_frame(&self) -> Option<&CapturedFrame> {
        self.stitch.as_ref()
    }

    /// Number of verified extensions since collection opened.
    #[must_use]
    pub const fn extensions(&self) -> u64 {
        self.extensions
    }

    /// Number of de-duplicated reverse passes since collection opened.
    #[must_use]
    pub const fn duplicates(&self) -> u64 {
        self.duplicates
    }

    /// Applies the input-protection installation result.
    ///
    /// Only valid while prepared; a success asks the app for the trusted first
    /// frame, a failure stops with a known-state refusal.
    pub fn install_input(&mut self, result: Result<(), ScrollInputError>) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Prepared {
            return ScrollPrepOutcome::Ignored;
        }
        match result {
            Ok(()) => ScrollPrepOutcome::CaptureFrame,
            Err(error) => self.stop_with(ScrollRefusal::Input(error)),
        }
    }

    /// Admits the trusted initial frame and opens collection.
    ///
    /// Only valid while prepared and awaiting the frame; a duplicate admission
    /// (the single-frame loop never queues a second raw frame) is ignored. The
    /// origin is the first verified stitch document.
    pub fn admit_frame(&mut self, frame: CapturedFrame) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Prepared {
            return ScrollPrepOutcome::Ignored;
        }
        self.stitch = Some(frame.clone());
        self.frame = Some(frame);
        self.phase = ScrollPhase::Collecting;
        ScrollPrepOutcome::Collecting
    }

    /// Accepts one raw frame captured during collection for verification.
    ///
    /// Only valid while collecting; the frame is held until the app runs the
    /// image verifier and reports the decision.
    pub fn collect_frame(&mut self, frame: CapturedFrame) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Collecting {
            return ScrollPrepOutcome::Ignored;
        }
        self.pending_frame = Some(frame);
        ScrollPrepOutcome::VerifyFrame
    }

    /// Takes the raw frame awaiting verification, if any.
    #[must_use]
    pub fn take_pending_frame(&mut self) -> Option<CapturedFrame> {
        self.pending_frame.take()
    }

    /// Stores a strictly verified stitched document.
    pub fn verify_stitch(&mut self, frame: CapturedFrame, extended: bool) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Collecting {
            return ScrollPrepOutcome::Ignored;
        }
        self.stitch = Some(frame);
        if extended {
            self.extensions += 1;
        } else {
            self.duplicates += 1;
        }
        ScrollPrepOutcome::StitchUpdated
    }

    /// Stops collection because a frame could not be verified.
    pub fn refuse_stitch(&mut self, error: ScrollStitchError) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Collecting {
            return ScrollPrepOutcome::Ignored;
        }
        self.stop_with(ScrollRefusal::Stitch(error))
    }

    /// Stops with a known-state refusal from collection (protection or target loss).
    pub fn stop_collecting(&mut self, refusal: ScrollRefusal) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Collecting {
            return ScrollPrepOutcome::Ignored;
        }
        self.stop_with(refusal)
    }

    /// Stops with a known-state refusal from the frame-admission handshake.
    pub fn stop_prepared(&mut self, refusal: ScrollRefusal) -> ScrollPrepOutcome {
        if self.phase != ScrollPhase::Prepared {
            return ScrollPrepOutcome::Ignored;
        }
        self.stop_with(refusal)
    }

    /// The session tap was disabled while armed or collecting.
    pub fn lose_input_protection(&mut self) -> ScrollPrepOutcome {
        if !matches!(self.phase, ScrollPhase::Prepared | ScrollPhase::Collecting) {
            return ScrollPrepOutcome::Ignored;
        }
        self.stop_with(ScrollRefusal::Input(ScrollInputError::ProtectionLost))
    }

    /// Stashes the region document while a completion output runs.
    pub fn stash_region_document(&mut self, document: Box<Document>) {
        self.region_document = Some(document);
    }

    /// Takes the stashed region document back, if completion did not end the session.
    pub fn take_region_document(&mut self) -> Option<Box<Document>> {
        self.region_document.take()
    }

    /// Applies the real permission preflight result.
    pub fn check_permissions(&mut self, available: ScrollPermissions) -> ScrollPrepOutcome {
        self.permissions = available;
        if available.all_available() {
            self.phase = ScrollPhase::IdentifyingTarget;
            ScrollPrepOutcome::IdentifyTarget
        } else {
            let missing = available.missing();
            self.phase = ScrollPhase::Stopped;
            self.refusal = Some(ScrollRefusal::Permissions(missing.clone()));
            ScrollPrepOutcome::Refused(ScrollRefusal::Permissions(missing))
        }
    }

    /// Locks the identified target after validating it against the selection.
    pub fn lock_target(
        &mut self,
        result: Result<ScrollTarget, ScrollTargetError>,
    ) -> ScrollPrepOutcome {
        match result {
            Ok(target) if target.frame.contains(self.selection) => {
                self.target = Some(target);
                self.phase = ScrollPhase::PreparingWindows;
                ScrollPrepOutcome::PrepareWindows
            }
            Ok(_) => self.stop_with(ScrollRefusal::Target(ScrollTargetError::Geometry)),
            Err(error) => self.stop_with(ScrollRefusal::Target(error)),
        }
    }

    /// Fixes the own-window exclusion set.
    pub fn fix_windows(
        &mut self,
        result: Result<Vec<u32>, ScrollWindowError>,
    ) -> ScrollPrepOutcome {
        match result {
            Ok(ids) => {
                self.exclusion = Some(ids);
                self.phase = ScrollPhase::Prepared;
                ScrollPrepOutcome::Prepared
            }
            Err(error) => self.stop_with(ScrollRefusal::Windows(error)),
        }
    }

    /// Cancels preparation; the region session and selection are untouched.
    pub fn stop(&mut self) -> ScrollPrepOutcome {
        self.phase = ScrollPhase::Stopped;
        ScrollPrepOutcome::Stopped
    }

    fn stop_with(&mut self, refusal: ScrollRefusal) -> ScrollPrepOutcome {
        self.phase = ScrollPhase::Stopped;
        self.refusal = Some(refusal.clone());
        ScrollPrepOutcome::Refused(refusal)
    }
}

/// The single display+crop → frame-geometry conversion for Scroll captures.
///
/// The initial (and every later) Scroll frame is captured with `sourceRect` at
/// the fixed selection, so its geometry is the display-anchored rectangle of
/// the selection's physical crop. The app and the core share this one
/// conversion; an admitted frame whose geometry differs is not the fixed
/// selection and is refused.
///
/// # Errors
/// Returns the [`FrameError`] of [`ScreenGeometry::try_new`] when the crop
/// cannot become a valid 2x frame geometry (edges, pixel budget, arithmetic).
pub fn scroll_frame_geometry(
    region: ScreenGeometry,
    crop: PixelRect,
) -> Result<ScreenGeometry, FrameError> {
    let scale = region.scale();
    let [origin_x, origin_y] = region.screen_origin();
    ScreenGeometry::try_new(
        region.display_id(),
        origin_x + f64::from(crop.x()) / scale,
        origin_y + f64::from(crop.y()) / scale,
        f64::from(crop.width()) / scale,
        f64::from(crop.height()) / scale,
        scale,
    )
}
