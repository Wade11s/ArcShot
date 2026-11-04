//! Pure Scroll Capture preparation: permissions, target lock and own-window exclusion.
//!
//! This module is the authoritative, platform-neutral half of I-11 steps 1–2 and
//! the permission/target half of I-12. It never touches system I/O: the app
//! injects the real permission results, the identified target and the fixed
//! exclusion set, and this module decides whether preparation may continue,
//! stop without output, or require a relaunch.
use crate::{CaptureTicket, Selection};

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

/// Why Scroll preparation stopped without producing output.
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
}

/// Observable Scroll preparation lifecycle, independent of the region phase.
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
    /// Native preparation finished; the input handshake/first frame is a later slice.
    Prepared,
    /// Preparation stopped; the region session and its selection are kept.
    Stopped,
}

/// The state transition one preparation action produces.
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
    /// Preparation stopped with a known-state refusal; keep the region session.
    Refused(ScrollRefusal),
    /// Preparation was cancelled; keep the region session.
    Stopped,
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
}

impl ScrollPreparation {
    /// Starts preparation for one frozen selection.
    #[must_use]
    pub const fn new(ticket: CaptureTicket, selection: Selection) -> Self {
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
