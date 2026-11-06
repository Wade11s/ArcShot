//! Connects the authoritative session to safe native operations and GPUI windows.
use crate::{
    MenuBarGlobal,
    ui::{
        COMPLETION_PREVIEW_ENTER, COMPLETION_PREVIEW_EXIT, COMPLETION_PREVIEW_HOLD, CaptureView,
        CompletionPreviewView, HotkeyView, NoticeView, OverlayPreview, ScrollControlView,
        ScrollPermissionView, ScrollPreviewView, TextEditMirror, UiCommand, UiQueue,
    },
};
use arcshot_app::{
    CaptureSubmitError, CaptureTasks, DensityWriteOutcome, GuideHistory, HotkeyAction,
    HotkeyEffect, HotkeyFlow, HotkeyNotice, HotkeyOutcome, LoginItemOutcome, OutputSubmitError,
    OutputTasks, PARTIAL_INCOMPLETE_WARNING, PendingSave, PermissionAction, PermissionEffect,
    PermissionFlow, PreferencesAction, PreferencesEffect, PreferencesFlow, PreferencesNotice,
    RestartGate, ScrollFrameQueue, ScrollFrameSubmitError, ScrollFrameTasks, ScrollPartialChoice,
    ScrollPermissionAction, ScrollPermissionEffect, ScrollPermissionFlow, ScrollPermissionNotice,
    ScrollTapCommand, ScrollTargetTasks, encode_document, may_submit_scroll_frame, partial_action,
    preview_layout, scroll_tap_action,
};
use arcshot_core::{
    Annotation, AnnotationStyle, ArrowStyle, BlurStyle, CaptureAction, CaptureEffect,
    CaptureFailure, CapturePhase, CaptureSession, CaptureTicket, CapturedFrame, Density, Document,
    FrameError, LineStyle, OutputFailure, OutputJob, PixelRect, RectangleStyle, SavePanelResult,
    ScreenGeometry, ScrollInputError, ScrollPhase, ScrollRefusal, ScrollStitchError,
    ScrollTargetError, ScrollWindowError, Selection, TextStyle, Tool,
};
use arcshot_macos::{self as native, DisplaySnapshot, MenuCommand};
use gpui::{
    App, AppContext, Bounds, Context, Point, RenderImage, Task, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, px, size,
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

/// UI polling cadence, not a capture sampling frequency or a performance assertion.
const POLL_INTERVAL: Duration = Duration::from_millis(16);
/// Allows the preparing window to be registered before exact native window matching.
const PREPARE_SETTLE: Duration = Duration::from_millis(80);
/// Allows the Scroll control window to register before the exclusion set is fixed.
const SCROLL_WINDOW_SETTLE: Duration = Duration::from_millis(80);
/// Heading of a capture failure notice; it is not a settings problem.
const CAPTURE_ERROR_HEADING: &str = "无法继续截图";
/// Heading of a menu-preference failure notice; neutral to the failing setting.
const PREFERENCE_ERROR_HEADING: &str = "无法更新偏好设置";
/// Control status after native preparation; input protection comes next.
const SCROLL_PREPARED_STATUS: &str = "滚动截图已准备";
/// Control status while preparation is running.
const SCROLL_PREPARING_STATUS: &str = "正在准备滚动截图…";
/// Control status once the trusted first frame admitted collection (I-11 §5).
const SCROLL_COLLECTING_STATUS: &str = "可以滚动";
/// Operation hint shown with the collecting status; no percentage, ETA or pause.
const SCROLL_COLLECTING_HINT: &str = "上下滚动采集内容；双击或按回车完成，Esc 取消";
/// Operation hint after a failed completion; retry and abandon stay explicit.
const SCROLL_RETRY_HINT: &str = "回车重试复制，Esc 放弃并返回选区";
/// Operation hint after a failed partial publish; the choice stays explicit.
const SCROLL_PARTIAL_RETRY_HINT: &str = "复制失败，已验证部分仍在；可重试或放弃返回选区";
/// Deadline for bringing the locked target to the foreground after preparation.
const SCROLL_ACTIVATE_DEADLINE: Duration = Duration::from_secs(3);
/// Cadence of the public target-integrity re-check during collection; not a
/// performance assertion, just a slow poll of public window state.
const SCROLL_TARGET_CHECK_INTERVAL: Duration = Duration::from_millis(500);
/// Fixed live preview size in logical points (I-11 §5). The window is opened
/// once during preparation and never moved or rebuilt during collection.
const SCROLL_PREVIEW_WIDTH: f64 = 600.0;
const SCROLL_PREVIEW_HEIGHT: f64 = 400.0;
const SCROLL_PREVIEW_MARGIN: f64 = 12.0;
/// V1 cap on the preview image itself, in physical pixels (I-13).
const SCROLL_PREVIEW_MAX_PIXELS: usize = arcshot_image::MAX_PREVIEW_PIXELS;
/// Presentation-only throttle for the fixed live preview refresh.
const SCROLL_PREVIEW_THROTTLE: Duration = Duration::from_millis(80);

/// One deferred input-protection installation after native preparation.
#[derive(Debug)]
struct PendingScrollInput {
    ticket: CaptureTicket,
    started: Instant,
}

/// Which core handshake owns a frame request and classifies its failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollFrameRoute {
    /// The trusted initial frame, which opens collection.
    Initial,
    /// A later raw frame, which must be verified against history.
    Collected,
}

struct Coordinator {
    session: CaptureSession,
    tasks: CaptureTasks,
    output_tasks: OutputTasks,
    /// Destination chosen by the native save panel, fenced by its exact job.
    pending_save: PendingSave,
    /// Job whose save panel is still open; its outcome is drained from AppKit.
    pending_panel: Option<OutputJob>,
    permissions: PermissionFlow,
    hotkeys: Option<native::GlobalHotkeys>,
    hotkey_flow: HotkeyFlow,
    hotkey_settings: Option<WindowHandle<HotkeyView>>,
    hotkey_view_error: Option<String>,
    hotkey_error: Option<String>,
    preferences: PreferencesFlow,
    /// Preference failure text waiting for a window without an active session.
    preferences_error: Option<String>,
    /// Whether the current notice window belongs to a preference failure.
    settings_notice: bool,
    queue: UiQueue,
    overlay: Option<WindowHandle<CaptureView>>,
    overlay_error: Option<String>,
    notice: Option<WindowHandle<NoticeView>>,
    pending: Option<(CaptureTicket, Instant, Option<u32>)>,
    /// True while the frozen overlay still needs one active-application check.
    overlay_activation_pending: bool,
    /// The built-in display geometry captured with the region frame. Scroll
    /// completion swaps in a smaller frame document, so display validation must
    /// stay anchored to this original geometry (I-05/I-12 display loss).
    region_geometry: Option<ScreenGeometry>,
    preview_task: Option<Task<()>>,
    started: Option<Instant>,
    settings_departed: bool,
    visiting_settings: bool,
    preview: PreviewSync,
    /// Scroll-only permission notice policy and its window.
    scroll_permissions: ScrollPermissionFlow,
    scroll_notice: Option<WindowHandle<ScrollPermissionView>>,
    /// The minimal control UI that stays visible during Scroll preparation.
    scroll_control: Option<WindowHandle<ScrollControlView>>,
    /// The fixed live verified preview; it never moves or rebuilds during
    /// collection, only its content updates in place (I-11 §5).
    scroll_preview: Option<WindowHandle<ScrollPreviewView>>,
    /// Strict vertical verifier/compositor for the current collection.
    scroll_stitch: Option<arcshot_image::StitchBuilder>,
    /// Bounded raw frames awaiting verification (backpressure, never drops).
    scroll_raw_queue: ScrollFrameQueue,
    /// Where the fixed preview was placed: right, left or inner bottom-right.
    scroll_preview_place: &'static str,
    /// Exact public window number of the fixed preview window, read from its
    /// live NSView before collection opens.
    scroll_preview_id: Rc<RefCell<Option<u32>>>,
    /// Throttle for in-place preview refreshes; verification itself is never
    /// throttled. This is presentation cadence, not a capture sampling rate.
    scroll_preview_last: Option<Instant>,
    /// Bounded worker for the Scroll target probe.
    scroll_target_tasks: ScrollTargetTasks,
    /// Deferred own-window exclusion probe; gives the control window time to register.
    pending_scroll_exclusion: Option<(CaptureTicket, Instant)>,
    /// Deferred input-protection installation (target activation + tap).
    pending_scroll_input: Option<PendingScrollInput>,
    /// Bounded worker for the Scroll trusted-initial-frame request.
    scroll_frame_tasks: ScrollFrameTasks,
    /// Exact public window number of the Scroll control window, read from its
    /// GPUI NSView at creation; it identifies the only own UI events may reach.
    scroll_control_id: Rc<RefCell<Option<u32>>>,
    /// The installed Quartz session tap and the preparation owning it.
    scroll_tap: Option<(CaptureTicket, native::ScrollInputTap)>,
    /// A capture restart waiting for the Scroll frame slot to settle.
    restart_gate: RestartGate,
    /// When the public target-integrity check last ran during collection.
    scroll_target_checked: Option<Instant>,
    /// Region overlay to restore when a Scroll permission notice closes.
    scroll_restore: Option<CaptureTicket>,
    /// The confirmed-copy whole-image thumbnail; it leaves after its fixed stay.
    completion_preview: Option<WindowHandle<CompletionPreviewView>>,
    /// When the completion thumbnail must leave; `Instant` is app presentation
    /// timing, never a capture or session semantic.
    completion_preview_deadline: Option<(CaptureTicket, Instant)>,
    /// Encoded PNG handed to the completion preview by the successful copy.
    completion_preview_png: Option<Vec<u8>>,
    /// The fixed exclusion set for the current Scroll preparation (#31).
    scroll_exclusion: Vec<u32>,
    /// The still-visible subset of the fixed set, fixed immediately before
    /// collection. A dismissed overlay may leave the WindowServer between
    /// preparation and collection; no window may ever be added to this set.
    scroll_capture_exclusion: Vec<u32>,
    /// Receipt instant of the current Scroll activation, for PERF-SCROLL-START.
    scroll_started: Option<Instant>,
    /// Receipt instant of the newest raw frame submission, for PERF-FRAME-PREVIEW.
    scroll_frame_submitted: Option<Instant>,
    /// (submission, stitch-decision) instants for the verified frame whose
    /// preview update is pending, for PERF-FRAME-PREVIEW.
    scroll_frame_latency: Option<(Instant, Instant)>,
    /// Receipt instant of the newest scroll event passed to the target, for
    /// PERF-EVENT-PREVIEW.
    scroll_event_started: Option<Instant>,
    /// Scroll event already reported, so one event is never timed twice.
    scroll_event_reported: Option<Instant>,
    /// Whether this capture already reported its first overlay presentation.
    ///
    /// A frozen overlay is presented again when Scroll returns to the region
    /// session; timing that second presentation from the original start would
    /// report the whole intervening session as capture latency.
    wake_reported: bool,
    scroll_visiting_settings: bool,
    scroll_settings_departed: bool,
}

/// One composed selection key: the fenced crop and its annotation snapshot.
type PreviewKey = (PixelRect, Vec<Annotation>);

/// Coalesces background annotation previews to at most one in flight.
#[derive(Default)]
struct PreviewSync {
    in_flight: Option<PreviewKey>,
    applied: Option<PreviewKey>,
}
pub(crate) fn launch(cx: &mut App) {
    let hotkeys = native::GlobalHotkeys::install().ok();
    let hotkey_flow = HotkeyFlow::new(native::hotkey_settings_load());
    let completion_sound = native::completion_sound_enabled();
    // A launch read failure has no user intent to protect: the documented
    // Compact default is used and reported. An interactive toggle whose read
    // fails goes through the in-product notice path instead.
    let density = native::density().unwrap_or_else(|error| {
        log::warn!("density_launch_read_failure={error}");
        Density::Compact
    });
    log::info!(
        "hotkey_launch config={} adapter={:?}",
        hotkey_flow.config(),
        hotkeys.is_some()
    );
    // The persisted menu preferences are the only authority for a silent
    // success and for the toolbar density.
    log::info!("preferences_launch completion_sound={completion_sound} density={density:?}");
    let coordinator = cx.new(|_| Coordinator {
        session: CaptureSession::default(),
        tasks: CaptureTasks::default(),
        output_tasks: OutputTasks::default(),
        pending_save: PendingSave::default(),
        pending_panel: None,
        permissions: PermissionFlow::new(if native::permission_guide_seen() {
            GuideHistory::Handled
        } else {
            GuideHistory::Unseen
        }),
        hotkeys,
        hotkey_flow,
        hotkey_settings: None,
        hotkey_view_error: None,
        hotkey_error: None,
        preferences: PreferencesFlow::new(completion_sound, density),
        preferences_error: None,
        settings_notice: false,
        queue: Rc::new(RefCell::new(Vec::new())),
        overlay: None,
        overlay_error: None,
        notice: None,
        pending: None,
        overlay_activation_pending: false,
        region_geometry: None,
        preview_task: None,
        started: None,
        settings_departed: false,
        visiting_settings: false,
        preview: PreviewSync::default(),
        scroll_permissions: ScrollPermissionFlow::default(),
        scroll_notice: None,
        scroll_control: None,
        scroll_preview: None,
        scroll_stitch: None,
        scroll_raw_queue: ScrollFrameQueue::new(),
        scroll_preview_place: "none",
        scroll_preview_id: Rc::new(RefCell::new(None)),
        scroll_preview_last: None,
        scroll_target_tasks: ScrollTargetTasks::default(),
        pending_scroll_exclusion: None,
        pending_scroll_input: None,
        scroll_frame_tasks: ScrollFrameTasks::default(),
        scroll_control_id: Rc::new(RefCell::new(None)),
        scroll_tap: None,
        restart_gate: RestartGate::default(),
        scroll_target_checked: None,
        scroll_restore: None,
        scroll_exclusion: Vec::new(),
        scroll_capture_exclusion: Vec::new(),
        scroll_visiting_settings: false,
        scroll_settings_departed: false,
        scroll_started: None,
        scroll_frame_submitted: None,
        scroll_frame_latency: None,
        scroll_event_started: None,
        scroll_event_reported: None,
        wake_reported: false,
        completion_preview: None,
        completion_preview_deadline: None,
        completion_preview_png: None,
    });
    coordinator.update(cx, |state, cx| {
        state
            .session
            .set_completion_sound(state.preferences.completion_sound());
        state.session.set_density(state.preferences.density());
        cx.global::<MenuBarGlobal>()
            .0
            .set_density_state(state.preferences.density());
        state.permission(PermissionAction::Launch, cx);
        state.hotkey(HotkeyAction::Launch, cx);
        state.preference(PreferencesAction::Launch, cx);
    });
    // The foreground task owns the coordinator; workers never own GPUI or AppKit state.
    //
    // The task keeps the strong handle on purpose: `Entity::update` on `AsyncApp` no
    // longer reports a released entity, and a weak handle would release the
    // coordinator as soon as this closure is the only remaining owner. The loop is
    // therefore tied to the app lifetime, not to an entity-liveness probe: GPUI stops
    // polling foreground tasks once the app is torn down after `cx.quit()`.
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            coordinator.update(cx, |state, cx| state.tick(cx));
        }
    })
    .detach();
}

impl Coordinator {
    fn busy(&self) -> bool {
        matches!(
            self.session.phase(),
            CapturePhase::Preparing | CapturePhase::Draining
        )
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let commands = cx.global_mut::<MenuBarGlobal>().0.drain_commands();
        for command in commands {
            match command {
                MenuCommand::StartCapture => self.start_capture(cx),
                MenuCommand::HotkeySettings => {
                    log::info!("menu_hotkey_settings");
                    self.open_hotkey_settings(cx);
                }
                MenuCommand::ScreenRecordingPermissions if !self.busy() => {
                    self.close_scroll_ui(cx);
                    self.action(CaptureAction::Cancel, cx);
                    self.permission(PermissionAction::Check, cx);
                }
                MenuCommand::ToggleDensity => self.preference(PreferencesAction::ToggleDensity, cx),
                MenuCommand::ToggleCompletionSound => {
                    self.preference(PreferencesAction::ToggleCompletionSound, cx)
                }
                MenuCommand::ToggleLaunchAtLogin => {
                    self.preference(PreferencesAction::ToggleLoginItem, cx)
                }
                _ => {}
            }
        }
        // The global hotkey enters the same Capture Session as the menu command.
        if self
            .hotkeys
            .as_ref()
            .is_some_and(native::GlobalHotkeys::take_pressed)
        {
            self.start_capture(cx);
        }
        let queued = std::mem::take(&mut *self.queue.borrow_mut());
        for command in queued {
            match command {
                UiCommand::CancelCapture(ticket) => {
                    self.action(CaptureAction::CancelAttempt(ticket), cx)
                }
                UiCommand::SelectRegion { ticket, from, to } => {
                    if self.session.may_present(ticket) {
                        self.overlay_error = None;
                        self.action(CaptureAction::SelectRegion { ticket, from, to }, cx);
                    }
                }
                UiCommand::PointerPressed { ticket, point } if self.session.may_present(ticket) => {
                    self.overlay_error = None;
                    self.action(CaptureAction::PointerPressed { ticket, point }, cx);
                }
                UiCommand::PointerMoved { ticket, point } if self.session.may_present(ticket) => {
                    self.action(CaptureAction::PointerMoved { ticket, point }, cx);
                }
                UiCommand::PointerReleased(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::PointerReleased { ticket }, cx);
                }
                UiCommand::SelectTool(ticket, tool) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::SelectTool { ticket, tool }, cx);
                }
                UiCommand::RectangleOption(ticket, option) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::RectangleOption { ticket, option }, cx);
                }
                UiCommand::LineOption(ticket, option) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::LineOption { ticket, option }, cx);
                }
                UiCommand::ArrowOption(ticket, option) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::ArrowOption { ticket, option }, cx);
                }
                UiCommand::BlurOption(ticket, option) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::BlurOption { ticket, option }, cx);
                }
                UiCommand::TextOption(ticket, option) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::TextOption { ticket, option }, cx);
                }
                UiCommand::BeginText { ticket, point } if self.session.may_present(ticket) => {
                    self.action(CaptureAction::TextBegin { ticket, point }, cx);
                }
                UiCommand::TextEdited {
                    ticket,
                    content,
                    bounds,
                } if self.session.may_present(ticket) => {
                    self.action(
                        CaptureAction::TextEdited {
                            ticket,
                            content,
                            bounds,
                        },
                        cx,
                    );
                }
                UiCommand::TextCommit(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::TextCommit { ticket }, cx);
                }
                UiCommand::DeleteObject(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::DeleteSelected { ticket }, cx);
                }
                UiCommand::DeselectSelected(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::DeselectSelected { ticket }, cx);
                }
                UiCommand::Undo(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::Undo { ticket }, cx);
                }
                UiCommand::Redo(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::Redo { ticket }, cx);
                }
                UiCommand::FinishCapture { ticket, point } => {
                    if self.session.may_present(ticket) {
                        self.overlay_error = None;
                        self.action(
                            match point {
                                Some(point) => CaptureAction::FinishAt { ticket, point },
                                None => CaptureAction::Finish { ticket },
                            },
                            cx,
                        );
                    }
                }
                UiCommand::SaveCapture(ticket) => {
                    if self.session.may_present(ticket) {
                        self.overlay_error = None;
                        self.action(CaptureAction::Save { ticket }, cx);
                    }
                }
                UiCommand::EnterScroll(ticket) => {
                    if self.session.may_present(ticket) {
                        self.overlay_error = None;
                        self.action(CaptureAction::EnterScroll { ticket }, cx);
                    }
                }
                UiCommand::ScrollPermission(window_id, action)
                    if self
                        .scroll_notice
                        .is_some_and(|notice| notice.window_id() == window_id) =>
                {
                    self.scroll_permission(action, cx);
                }
                UiCommand::ScrollCancel(ticket) => {
                    if self.session.scroll_phase() != ScrollPhase::Inactive {
                        self.close_scroll_control(cx);
                        self.action(CaptureAction::CancelScroll { ticket }, cx);
                    }
                }
                UiCommand::EnterCompletionAnnotation(ticket)
                    if self.session.may_present(ticket)
                        && self.session.completion_preview().is_some() =>
                {
                    self.overlay_error = None;
                    self.close_completion_preview(cx);
                    self.action(CaptureAction::EnterCompletionAnnotation { ticket }, cx);
                }
                UiCommand::AddDefaultRectangle(ticket) if self.session.may_present(ticket) => {
                    self.action(CaptureAction::AddDefaultRectangle { ticket }, cx);
                }
                UiCommand::ScrollPartialAccept(ticket) => {
                    if self.session.scroll_partial_available() {
                        log::info!("scroll_partial_choice action=accept ticket={ticket:?}");
                        self.action(
                            partial_action(ScrollPartialChoice::UseVerifiedPartial, ticket),
                            cx,
                        );
                    }
                }
                UiCommand::ScrollPartialAbandon(ticket) => {
                    if self.session.scroll_partial_available() {
                        log::info!("scroll_partial_choice action=abandon ticket={ticket:?}");
                        self.action(partial_action(ScrollPartialChoice::Abandon, ticket), cx);
                    }
                }
                UiCommand::ScrollControlClosed(window_id, ticket) => {
                    if self
                        .scroll_control
                        .is_some_and(|control| control.window_id() == window_id)
                    {
                        self.close_scroll_control(cx);
                        if self.session.scroll_phase() != ScrollPhase::Inactive {
                            self.action(CaptureAction::CancelScroll { ticket }, cx);
                        }
                    }
                }
                UiCommand::Permission(window_id, action)
                    if !self.busy()
                        && self
                            .notice
                            .is_some_and(|notice| notice.window_id() == window_id) =>
                {
                    if self.settings_notice {
                        // The preference failure notice only offers dismissal;
                        // the menu itself stays available for a retry.
                        self.preference(PreferencesAction::DismissNotice, cx);
                    } else {
                        self.permission(action, cx);
                    }
                }
                UiCommand::HotkeySave(window_id, candidate)
                    if self
                        .hotkey_settings
                        .is_some_and(|settings| settings.window_id() == window_id) =>
                {
                    log::info!("hotkey_save_requested candidate={candidate}");
                    // Saving while recording keeps the old registration
                    // suspended: the flow registers the candidate first and
                    // restores the suspended value if registration fails.
                    self.hotkey(HotkeyAction::Save(candidate), cx);
                }
                UiCommand::HotkeyRecording(window_id, true)
                    if self
                        .hotkey_settings
                        .is_some_and(|settings| settings.window_id() == window_id) =>
                {
                    self.hotkey(HotkeyAction::RecordingStarted, cx);
                }
                UiCommand::HotkeyRecording(window_id, false)
                | UiCommand::HotkeySettingsClosed(window_id)
                    if self
                        .hotkey_settings
                        .is_some_and(|settings| settings.window_id() == window_id) =>
                {
                    if matches!(command, UiCommand::HotkeySettingsClosed(_)) {
                        self.close_hotkey_settings(cx);
                    }
                    self.hotkey(HotkeyAction::RecordingEnded, cx);
                    self.hotkey(HotkeyAction::SettingsClosed, cx);
                }
                _ => {}
            }
        }
        // The completion thumbnail leaves after exactly its documented stay;
        // hover never extends it and cancelling keeps the earlier clipboard copy.
        if let Some((ticket, deadline)) = self.completion_preview_deadline
            && self.completion_preview.is_some()
            && Instant::now() >= deadline
        {
            log::info!("completion_preview_expired ticket={ticket:?}");
            self.close_completion_preview(cx);
            self.action(CaptureAction::CancelAttempt(ticket), cx);
        }
        if let Some(error) = self.hotkey_error.take_if(|_| {
            self.notice.is_none() && self.overlay.is_none() && self.hotkey_settings.is_none()
        }) {
            self.show_error(error, cx);
        }
        // A preference failure never steals the frozen session's window; it is
        // shown as soon as the mouse capture is gone and no notice is open.
        let busy = self.busy();
        if let Some(error) = self
            .preferences_error
            .take_if(|_| self.notice.is_none() && self.overlay.is_none() && !busy)
        {
            self.show_preferences_error(error, cx);
        }
        if self.visiting_settings && !self.busy() {
            if !native::application_active() {
                self.settings_departed = true;
            }
            if self.settings_departed && native::application_active() {
                self.visiting_settings = false;
                self.permission(PermissionAction::ReturnedFromSettings, cx);
            }
        }
        // The Scroll permission flow has its own Settings-return detection: a
        // successful re-check never auto-starts Scroll, it only updates the notice.
        // The session's Scroll phase is already Inactive after a permission
        // refusal, so the detection must not depend on it.
        if self.scroll_visiting_settings {
            if !native::application_active() {
                self.scroll_settings_departed = true;
            }
            if self.scroll_settings_departed && native::application_active() {
                self.scroll_visiting_settings = false;
                self.scroll_permission(ScrollPermissionAction::ReturnedFromSettings, cx);
            }
        }
        if let Some((ticket, ready)) = self.pending_scroll_exclusion
            && Instant::now() >= ready
        {
            self.pending_scroll_exclusion = None;
            if self.session.scroll_phase() == ScrollPhase::PreparingWindows {
                let result = native::scroll_exclusion_set();
                self.action(CaptureAction::ScrollWindowsPrepared { ticket, result }, cx);
            }
        }
        // Input protection: bring the locked target to the foreground, then
        // install the session tap. Bounded by an absolute deadline.
        if let Some(pending) = self.pending_scroll_input.take() {
            self.drive_scroll_input(pending, cx);
        }
        if let Some((_, tap)) = self.scroll_tap.as_ref()
            && let Some(scrolled) = tap.last_scroll()
            && self.scroll_event_reported != Some(scrolled)
        {
            self.scroll_event_started = Some(scrolled);
        }
        // The session tap reports raw user intents and lost protection; the
        // pure policy maps them into authoritative actions.
        let tap_report = self
            .scroll_tap
            .as_ref()
            .and_then(|(ticket, tap)| tap.take_command().map(|command| (*ticket, command)));
        if let Some((ticket, command)) = tap_report {
            log::info!("scroll_tap_command ticket={ticket:?} command={command:?}");
            match command {
                native::TapGateCommand::ProtectionLost => {
                    log::warn!("scroll_input_protection_lost");
                    self.action(CaptureAction::ScrollInputLost { ticket }, cx);
                }
                command => {
                    let command = match command {
                        native::TapGateCommand::Finish => ScrollTapCommand::Finish,
                        native::TapGateCommand::Cancel => ScrollTapCommand::Cancel,
                        native::TapGateCommand::ProtectionLost => unreachable!("handled above"),
                    };
                    if let Some(action) = scroll_tap_action(
                        self.session.scroll_phase(),
                        self.session.phase(),
                        self.session.output_busy(),
                        ticket,
                        command,
                    ) {
                        self.action(action, cx);
                    }
                }
            }
        }
        if let Some((ticket, result)) = self.scroll_frame_tasks.poll(Instant::now()) {
            if self.session.scroll_phase() == ScrollPhase::Prepared {
                // The trusted initial frame opens collection; it never enters
                // the raw queue or the verifier.
                self.action(CaptureAction::ScrollFrameAdmitted { ticket, result }, cx);
            } else {
                self.handle_scroll_frame_result(ticket, result, cx);
            }
        }
        // A restart deferred by an in-flight Scroll request resumes only once
        // that request has settled, so no two native requests ever coexist.
        if self.restart_gate.resume(self.scroll_frame_tasks.busy()) {
            self.start_capture(cx);
        }
        // One bounded request at a time, at most two raw frames queued. New
        // requests are submitted only while collection is live and no output
        // owns the completion snapshot (I-13 backpressure).
        self.pump_scroll_frames(cx);
        // Slow public re-check of the locked target during collection: the
        // target must stay on screen, unchanged and in the foreground.
        if self.session.scroll_phase() == ScrollPhase::Collecting
            && !self.session.output_busy()
            && let Some(ticket) = self.scroll_tap.as_ref().map(|(ticket, _)| *ticket)
            && self
                .scroll_target_checked
                .is_none_or(|checked| checked.elapsed() >= SCROLL_TARGET_CHECK_INTERVAL)
        {
            self.scroll_target_checked = Some(Instant::now());
            let target = self.session.scroll_target().cloned();
            let display = native::builtin_display().ok();
            if let (Some(target), Some(display)) = (target, display)
                && let Err(error) = native::scroll_target_intact(&target, display)
            {
                log::info!("scroll_target_intact=false reason={error:?}");
                self.action(CaptureAction::ScrollTargetLost { ticket, error }, cx);
            }
        }
        // Collection pauses input protection while a completion output runs
        // and re-opens when the collected result survives a failed publish.
        if let Some((_, tap)) = self.scroll_tap.as_ref() {
            match (self.session.scroll_phase(), self.session.output_busy()) {
                (ScrollPhase::Collecting, true) => tap.begin_drain(),
                (ScrollPhase::Collecting, false) => tap.reopen_collection(),
                _ => {}
            }
        }
        if let Some((ticket, result)) = self.scroll_target_tasks.poll(Instant::now()) {
            match &result {
                Ok(target) => log::info!(
                    "scroll_target window_id={} owner={} frame=[{},{},{},{}]",
                    target.window_id,
                    target.owner,
                    target.frame.x,
                    target.frame.y,
                    target.frame.width,
                    target.frame.height
                ),
                Err(error) => log::info!("scroll_target_failure reason={error:?}"),
            }
            match result {
                Err(ScrollTargetError::NativeUncertain) => {
                    self.action(CaptureAction::ScrollNativeUncertain { ticket }, cx);
                }
                other => {
                    self.action(
                        CaptureAction::ScrollTargetIdentified {
                            ticket,
                            result: other,
                        },
                        cx,
                    );
                }
            }
        }
        if let Some((ticket, ready, window_number)) = self.pending {
            // The preparing window only needs to be registered in the
            // WindowServer for the capture to exclude it; polling for that
            // exact condition is both faster and more precise than a fixed
            // sleep, which remains the upper bound.
            let registered = window_number.is_some_and(|number| {
                native::owned_visible_window_ids().is_ok_and(|ids| ids.contains(&number))
            });
            if registered || Instant::now() >= ready {
                self.pending = None;
                if self.session.may_capture(ticket) {
                    self.submit(ticket, cx);
                } else {
                    // Cancellation happened before any native request was submitted.
                    self.action(
                        CaptureAction::Completed {
                            ticket,
                            result: Err(CaptureFailure::WorkerUnavailable),
                        },
                        cx,
                    );
                }
            }
        }
        if let Some((ticket, result)) = self.tasks.poll(Instant::now()) {
            log::info!(
                "capture_result_us={}",
                self.started
                    .map(|s| s.elapsed().as_micros())
                    .unwrap_or_default()
            );
            cx.global_mut::<MenuBarGlobal>().0.restore_after_capture();
            self.action(CaptureAction::Completed { ticket, result }, cx);
        }
        if let Some((job, encoded)) = self.output_tasks.poll() {
            self.publish_output(job, encoded, cx);
        }
        if let Some(outcome) = native::take_save_panel_outcome() {
            self.resolve_save_panel(outcome, cx);
        }
        // The overlay takes keyboard focus after it opens; report the first tick
        // where the app really owns focus, so E2E can gate headed key delivery on
        // a product observation instead of a guess.
        if self.overlay_activation_pending && native::application_active() {
            self.overlay_activation_pending = false;
            log::info!("app_active surface=frozen");
        }
        // No SCK call is made for this continuous screen validity check. It is
        // anchored to the original region display geometry, so a Scroll
        // completion document (a smaller captured frame) never looks like a
        // display change and cancels the session mid-output.
        if let Some(expected) = self.region_geometry
            && !native::builtin_display()
                .ok()
                .and_then(|d| geometry(d).ok())
                .is_some_and(|d| d == expected)
        {
            self.action(CaptureAction::DisplayInvalidated, cx);
        }
        // While awaiting the partial choice, input protection has ended:
        // retire the tap (drain, then drop with counters) but keep the
        // control window, the verified preview and the stitch builder so
        // the user can still accept or abandon the verified result.
        if self.session.scroll_phase() == ScrollPhase::AwaitingPartialChoice {
            self.retire_scroll_tap();
            self.scroll_raw_queue.clear();
        }
        // A cancelled or invalidated session must not leave a Scroll control
        // window or session tap behind. A permission notice or a deferred
        // restore owns the screen and keeps its own window. The tap first
        // drains the input it owns, then retires with its counters logged.
        if self.session.scroll_phase() == ScrollPhase::Inactive
            && self.scroll_restore.is_none()
            && self.scroll_notice.is_none()
        {
            self.close_scroll_control(cx);
            self.close_scroll_preview(cx);
            self.pending_scroll_input = None;
            self.pending_scroll_exclusion = None;
            self.scroll_target_checked = None;
            self.scroll_raw_queue.clear();
            self.scroll_stitch = None;
            self.scroll_preview_place = "none";
            self.scroll_preview_last = None;
            self.scroll_preview_id.borrow_mut().take();
            self.retire_scroll_tap();
            // The status item was hidden for collection so no menu could open
            // behind the protection; it returns the moment Scroll is inactive.
            cx.global::<MenuBarGlobal>().0.restore_after_capture();
        }
    }

    /// The shared entry for the menu command and the global hotkey.
    fn start_capture(&mut self, cx: &mut Context<Self>) {
        // I-12/I-13: the single SCK slot must be free before a new capture may
        // start. Abandoning a Scroll session does not cancel a frame request
        // already in flight, so the restart waits for that request to settle
        // instead of opening a second concurrent native request.
        if !self.restart_gate.request(self.scroll_frame_tasks.busy()) {
            log::info!("capture_restart_deferred reason=scroll_frame_in_flight");
            return;
        }
        self.close_scroll_ui(cx);
        self.close_completion_preview(cx);
        self.completion_preview_png = None;
        self.permission(PermissionAction::Cancel, cx);
        self.started = Some(Instant::now());
        self.wake_reported = false;
        self.action(CaptureAction::Start, cx);
    }

    /// Drives the hotkey policy, executing its native and persistence effects.
    fn hotkey(&mut self, action: HotkeyAction, cx: &mut Context<Self>) {
        log::info!("hotkey_dispatch action={action:?}");
        let mut effects: VecDeque<_> = self.hotkey_flow.dispatch(action).into();
        while let Some(effect) = effects.pop_front() {
            match effect {
                HotkeyEffect::Register(value) => {
                    let outcome = self
                        .with_hotkeys(|hotkeys| hotkeys.set_active(value))
                        .map(|()| HotkeyOutcome::Applied)
                        .unwrap_or(HotkeyOutcome::Failed);
                    log::info!("hotkey_register value={value} outcome={outcome:?}");
                    effects.extend(
                        self.hotkey_flow
                            .dispatch(HotkeyAction::RegisterResult(outcome)),
                    );
                }
                HotkeyEffect::Suspend => {
                    let outcome = self
                        .with_hotkeys(native::GlobalHotkeys::suspend)
                        .map(|()| HotkeyOutcome::Applied)
                        .unwrap_or(HotkeyOutcome::Failed);
                    log::info!("hotkey_suspend outcome={outcome:?}");
                    effects.extend(
                        self.hotkey_flow
                            .dispatch(HotkeyAction::SuspendResult(outcome)),
                    );
                }
                HotkeyEffect::Resume => {
                    let outcome = self
                        .with_hotkeys(native::GlobalHotkeys::resume)
                        .map(|()| HotkeyOutcome::Applied)
                        .unwrap_or(HotkeyOutcome::Failed);
                    log::info!("hotkey_resume outcome={outcome:?}");
                    effects.extend(
                        self.hotkey_flow
                            .dispatch(HotkeyAction::ResumeResult(outcome)),
                    );
                }
                HotkeyEffect::Store(value) => {
                    log::info!("hotkey_store value={value}");
                    native::hotkey_settings_store(value);
                }
                HotkeyEffect::Notice(notice) => self.hotkey_notice(notice),
            }
        }
        // A failed suspension or a rejected save must leave recording mode;
        // otherwise the view would keep consuming keys with no hotkey suspended.
        let exit_recording =
            !matches!(action, HotkeyAction::RecordingStarted) || self.hotkey_view_error.is_some();
        self.refresh_hotkey_view(cx, exit_recording);
    }

    /// Drives the two I-04 menu preferences, executing their native effects.
    ///
    /// The completion-sound choice is persisted before the next output; every
    /// login-item decision is taken from, and confirmed by, a real system read.
    fn preference(&mut self, action: PreferencesAction, cx: &mut Context<Self>) {
        let mut source = match action {
            PreferencesAction::Launch => "launch",
            _ => "toggle",
        };
        let mut effects: VecDeque<_> = self.preferences.dispatch(action).into();
        while let Some(effect) = effects.pop_front() {
            match effect {
                PreferencesEffect::StoreCompletionSound(enabled) => {
                    native::store_completion_sound(enabled);
                    self.session.set_completion_sound(enabled);
                    cx.global::<MenuBarGlobal>()
                        .0
                        .set_completion_sound_state(enabled);
                    log::info!("completion_sound enabled={enabled} persisted=true");
                }
                PreferencesEffect::ReadLoginItemStatus => {
                    let state = native::login_item_status();
                    cx.global::<MenuBarGlobal>()
                        .0
                        .set_launch_at_login_state(state);
                    log::info!("login_item_status state={state:?} source={source}");
                    effects.extend(
                        self.preferences
                            .dispatch(PreferencesAction::LoginItemStatusRead(Some(state))),
                    );
                }
                PreferencesEffect::SetLoginItem(enabled) => {
                    let outcome = match native::set_login_item_enabled(enabled) {
                        Ok(()) => LoginItemOutcome::Applied,
                        Err(error) => {
                            log::error!("login_item_set_failure={error}");
                            LoginItemOutcome::Failed
                        }
                    };
                    log::info!("login_item_set enabled={enabled} outcome={outcome:?}");
                    // The next status read confirms or contradicts the request.
                    source = "verify";
                    effects.extend(
                        self.preferences
                            .dispatch(PreferencesAction::LoginItemSetResult(outcome)),
                    );
                }
                PreferencesEffect::Notice(notice) => {
                    log::warn!("preference_notice={notice:?}");
                    self.preferences_error = Some(preferences_notice_text(notice));
                }
                PreferencesEffect::ReadDensity => {
                    let read = native::density();
                    if let Err(error) = &read {
                        log::error!("density_read_failure={error}");
                    }
                    effects.extend(
                        self.preferences
                            .dispatch(PreferencesAction::DensityRead(read.ok())),
                    );
                }
                PreferencesEffect::StoreDensity(density) => {
                    let outcome = match native::store_density(density) {
                        Ok(()) => DensityWriteOutcome::Applied,
                        Err(error) => {
                            log::error!("density_write_failure={error}");
                            DensityWriteOutcome::Failed
                        }
                    };
                    effects.extend(
                        self.preferences
                            .dispatch(PreferencesAction::DensityWriteResult(outcome)),
                    );
                }
                PreferencesEffect::ReflectDensity(density) => {
                    // A confirmed change: mirror it in the authoritative session,
                    // the real menu state and one text diagnostic, then refresh
                    // any open overlay. No CaptureAction is dispatched, so the
                    // document (tool, styles, selection, objects, text) is kept.
                    self.session.set_density(density);
                    cx.global::<MenuBarGlobal>().0.set_density_state(density);
                    self.refresh_capture_view(cx);
                    log::info!("option_density density={density:?} persisted=true");
                }
            }
        }
        // A dismissed or retried failure releases its notice window and any
        // failure text that had not reached a window yet.
        if self.preferences.notice().is_none() {
            self.preferences_error = None;
            if self.settings_notice {
                self.close_notice(cx);
            }
        }
    }

    fn with_hotkeys(
        &mut self,
        operation: impl FnOnce(&mut native::GlobalHotkeys) -> Result<(), native::HotkeyError>,
    ) -> Option<()> {
        let hotkeys = self.hotkeys.as_mut()?;
        match operation(hotkeys) {
            Ok(()) => Some(()),
            Err(error) => {
                log::error!("hotkey_operation_failure={error}");
                None
            }
        }
    }

    fn hotkey_notice(&mut self, notice: HotkeyNotice) {
        let message = match notice {
            HotkeyNotice::StartupRegistrationFailed => format!(
                "全局快捷键 {} 注册失败，当前未生效。仍可从菜单栏选择“开始截图”，或打开“截图快捷键…”重新设置。",
                self.hotkey_flow.config()
            ),
            HotkeyNotice::SaveRejected => format!(
                "新组合注册失败，已保留 {}；可更换组合后重试。",
                self.hotkey_flow.config()
            ),
            HotkeyNotice::RestoreFailed => format!(
                "快捷键 {} 暂时无法恢复注册，当前未生效；可重新打开“截图快捷键…”重试，或从菜单栏开始截图。",
                self.hotkey_flow.config()
            ),
            HotkeyNotice::SuspendFailed => {
                "快捷键暂挂失败，已退出录入；可重新点击录入区域。".to_owned()
            }
        };
        // In-window notices stay in the settings window; the others surface
        // through the shared error notice once windows permit it.
        if matches!(
            notice,
            HotkeyNotice::SaveRejected | HotkeyNotice::SuspendFailed
        ) && self.hotkey_settings.is_some()
        {
            self.hotkey_view_error = Some(message);
        } else {
            self.hotkey_error = Some(message);
        }
    }

    fn refresh_hotkey_view(&mut self, cx: &mut Context<Self>, reset_recording: bool) {
        let Some(settings) = self.hotkey_settings else {
            return;
        };
        let current = self.hotkey_flow.config();
        let active = self.hotkey_flow.registered() == Some(current);
        let error = self.hotkey_view_error.take();
        let updated = settings.update(cx, |view, _, cx| {
            view.current = current;
            view.active = active;
            view.error = error;
            if reset_recording {
                view.recording = false;
            }
            cx.notify();
        });
        if updated.is_err() {
            // The window closed without our close command; drop the handle so
            // later refreshes do not keep targeting a missing window.
            self.hotkey_settings = None;
        }
    }

    fn open_hotkey_settings(&mut self, cx: &mut Context<Self>) {
        if self.hotkey_settings.is_some() {
            return;
        }
        let current = self.hotkey_flow.config();
        let active = self.hotkey_flow.registered() == Some(current);
        let queue = Rc::clone(&self.queue);
        let bounds = WindowBounds::centered(size(px(480.0), px(330.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let close_queue = Rc::clone(&queue);
                let window_id = gpui::Window::window_handle(window).window_id();
                window.on_window_should_close(cx, move |_, _| {
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::HotkeySettingsClosed(window_id));
                    true
                });
                let focus = cx.focus_handle();
                let entity = cx.new(|_| HotkeyView {
                    current,
                    active,
                    candidate: None,
                    recording: false,
                    error: None,
                    focus: focus.clone(),
                    queue,
                });
                window.focus(&focus, cx);
                entity
            },
        );
        match result {
            Ok(handle) => {
                self.hotkey_settings = Some(handle);
                cx.activate(true);
            }
            Err(error) => {
                self.hotkey_error = Some(format!("无法打开截图快捷键设置窗口：{error}"));
            }
        }
    }

    fn close_hotkey_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.hotkey_settings.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// Reports one semantic action to the capture authority.
    ///
    /// Every phase change is logged as one text line so end-to-end checks can
    /// observe session state without reading GPUI windows or pixels.
    fn dispatch(&mut self, action: CaptureAction) -> VecDeque<CaptureEffect> {
        let name = action_name(&action);
        let option_tool = action_style_tool(&action);
        let edits = matches!(
            action,
            CaptureAction::Undo { .. }
                | CaptureAction::Redo { .. }
                | CaptureAction::PointerPressed { .. }
                | CaptureAction::PointerReleased { .. }
                | CaptureAction::SelectRegion { .. }
                | CaptureAction::SelectTool { .. }
                | CaptureAction::RectangleOption { .. }
                | CaptureAction::LineOption { .. }
                | CaptureAction::ArrowOption { .. }
                | CaptureAction::BlurOption { .. }
                | CaptureAction::TextOption { .. }
                | CaptureAction::TextCommit { .. }
                | CaptureAction::DeleteSelected { .. }
                | CaptureAction::DeselectSelected { .. }
                | CaptureAction::AddDefaultRectangle { .. }
        );
        let before = self.session.phase();
        let scroll_before = self.session.scroll_phase();
        let is_scroll_start = matches!(action, CaptureAction::EnterScroll { .. });
        let dispatch_started = Instant::now();
        let effects: VecDeque<_> = self.session.dispatch(action).into();
        let dispatch_elapsed = dispatch_started.elapsed();
        let after = self.session.phase();
        let scroll_after = self.session.scroll_phase();
        if is_scroll_start {
            self.scroll_started = Some(dispatch_started);
        }
        if after != before {
            log::info!("capture_phase from={before:?} to={after:?} action={name}");
        }
        if scroll_after != scroll_before {
            log::info!("scroll_phase from={scroll_before:?} to={scroll_after:?} action={name}");
        }
        if edits && let Some(document) = self.session.document() {
            // Text-only edit diagnostics: version, geometry and style, never pixels.
            let selection = document.selection();
            let style = document
                .selected_index()
                .and_then(|index| document.annotations().get(index))
                .map(|annotation| annotation.style())
                .or_else(|| option_tool.and_then(|tool| document.tool_style(tool)))
                .or_else(|| document.tool_style(document.tool()))
                .map_or_else(|| "none".to_owned(), format_style);
            log::info!(
                "capture_edit action={name} version={} annotations={} selection={:?} tool={:?} selected={:?} style={}",
                document.version(),
                document.annotations().len(),
                selection.map(|selection| [
                    selection.x(),
                    selection.y(),
                    selection.width(),
                    selection.height()
                ]),
                document.tool(),
                document.selected_index(),
                style
            );
        }
        if edits {
            // Text-only interaction latency: command dequeue to session commit.
            log::info!(
                "interact_commit_us={} action={name}",
                dispatch_elapsed.as_micros()
            );
        }
        effects
    }

    fn action(&mut self, action: CaptureAction, cx: &mut Context<Self>) {
        // An explicit cancel withdraws a restart the user asked for while a
        // native request was still settling.
        if matches!(action, CaptureAction::Cancel) {
            self.restart_gate.cancel();
        }
        let mut effects = self.dispatch(action);
        while let Some(effect) = effects.pop_front() {
            match effect {
                CaptureEffect::CheckPermission(ticket) => {
                    effects.extend(self.dispatch(CaptureAction::PermissionChecked {
                        ticket,
                        available: native::screen_recording_available(),
                    }));
                }
                CaptureEffect::Capture(ticket) => {
                    if !self.session.may_capture(ticket) {
                        continue;
                    }
                    self.close_notice(cx);
                    match self.open_preparing(ticket, cx) {
                        Ok(window_number) => {
                            log::info!(
                                "capture_preparing_opened_us={}",
                                self.started
                                    .map(|s| s.elapsed().as_micros())
                                    .unwrap_or_default()
                            );
                            self.pending =
                                Some((ticket, Instant::now() + PREPARE_SETTLE, window_number));
                        }
                        Err(error) => {
                            self.show_error(format!("无法打开截图准备窗口：{error}"), cx);
                            effects.extend(self.dispatch(CaptureAction::Completed {
                                ticket,
                                result: Err(CaptureFailure::WorkerUnavailable),
                            }));
                        }
                    }
                }
                CaptureEffect::PresentFrozen(ticket) => {
                    if self.scroll_restore.take() == Some(ticket) {
                        // A Scroll permission notice owns the screen; the region
                        // overlay is restored when that notice closes.
                    } else {
                        self.prepare_preview(ticket, cx);
                    }
                }
                CaptureEffect::DismissOverlay => {
                    self.preview_task = None;
                    self.end_pending_save();
                    self.close_completion_preview(cx);
                    self.close_overlay(cx);
                }
                CaptureEffect::PermissionUnavailable => {
                    self.permission(PermissionAction::Unavailable, cx)
                }
                CaptureEffect::ReportFailure(error) => {
                    if matches!(error, CaptureFailure::DisplayInvalid) {
                        // Display loss cancels the whole session; no Scroll UI
                        // may outlive it.
                        self.close_scroll_ui(cx);
                    }
                    let message = if error.requires_relaunch() {
                        "原生捕获状态无法确认。已禁止输出和再次截图，请退出并重新打开 ArcShot。"
                    } else {
                        match error {
                            CaptureFailure::DisplayInvalid => {
                                "内建显示器已失效或几何/缩放发生变化，截图会话已取消。不会切换到外接屏幕。"
                            }
                            CaptureFailure::InvalidFrame => {
                                "捕获画面未通过 sRGB、不透明像素或尺寸校验。未产生输出，可从菜单重试。"
                            }
                            CaptureFailure::ResourceLimit => {
                                "截图超出尺寸/内存预算或无法分配图像。未产生输出，可从菜单重试。"
                            }
                            _ => "无法启动截图工作线程。未产生输出，可从菜单重试。",
                        }
                    };
                    self.show_error(message.into(), cx);
                }
                CaptureEffect::ComposeOutput(job) => {
                    self.overlay_error = None;
                    let Some(frame) = self.session.frame().cloned() else {
                        effects.extend(self.session.dispatch(CaptureAction::OutputCompleted {
                            job,
                            result: Err(OutputFailure::Worker),
                        }));
                        continue;
                    };
                    // Finish and explicit Save submit the same compose/encode
                    // route for the same snapshot; only the target differs.
                    let fence = job.clone();
                    let snapshot = job.clone();
                    let pixels = (snapshot.crop().width(), snapshot.crop().height());
                    let submitted = self.output_tasks.submit(fence, move || {
                        let started = Instant::now();
                        let encoded = encode_document(&frame, snapshot);
                        log::info!(
                            "output_encode_us={} pixels={}x{}",
                            started.elapsed().as_micros(),
                            pixels.0,
                            pixels.1
                        );
                        encoded
                    });
                    if let Err(error) = submitted {
                        log::error!("output_submit_failure={error}");
                        effects.extend(self.session.dispatch(CaptureAction::OutputCompleted {
                            job,
                            result: Err(match error {
                                OutputSubmitError::Busy => OutputFailure::Busy,
                                OutputSubmitError::WorkerUnavailable => OutputFailure::Worker,
                            }),
                        }));
                    }
                }
                CaptureEffect::RequestSavePanel(job) => {
                    self.overlay_error = None;
                    log::info!("save_panel_open version={}", job.version());
                    let opened = native::PngSavePanel::try_new("ArcShot.png")
                        .and_then(|panel| panel.begin());
                    match opened {
                        Ok(()) => self.pending_panel = Some(job),
                        Err(error) => {
                            log::error!("save_panel_failure={error}");
                            self.pending_panel = None;
                            effects.extend(self.dispatch(CaptureAction::SavePanelResolved {
                                job,
                                result: SavePanelResult::Failed,
                            }));
                        }
                    }
                }
                CaptureEffect::PlayCompletionSound => {
                    if !native::play_completion_sound() {
                        log::warn!("completion_sound_unavailable");
                    }
                }
                CaptureEffect::ShowCompletionPreview(ticket) => {
                    // Only a confirmed clipboard copy reaches this arm; the PNG
                    // was produced by the same shared encode path as the copy.
                    if let Some(png) = self.completion_preview_png.take()
                        && let Err(error) = self.open_completion_preview(ticket, png, cx)
                    {
                        log::error!("completion_preview_failure={error}");
                        effects.extend(self.dispatch(CaptureAction::CancelAttempt(ticket)));
                    }
                }
                CaptureEffect::PresentCompletionAnnotation(ticket) => {
                    if let Err(error) = self.open_completion_annotation(ticket, cx) {
                        log::error!("completion_annotation_failure={error}");
                        effects.extend(self.dispatch(CaptureAction::CancelAttempt(ticket)));
                    }
                }
                CaptureEffect::ReportOutputFailure(error) => {
                    log::error!("output_failure={error}");
                    if self.session.scroll_phase() == ScrollPhase::Collecting {
                        // A failed Scroll completion keeps the collected result:
                        // the control window carries the retry/abandon contract.
                        self.update_scroll_control_status(
                            &error.to_string(),
                            Some(SCROLL_RETRY_HINT),
                            cx,
                        );
                    } else if self.session.scroll_phase() == ScrollPhase::AwaitingPartialChoice {
                        // A failed partial publish is not a success: the
                        // verified result stays for an explicit retry/abandon
                        // and no success preview is shown.
                        self.update_scroll_control_partial(
                            &error.to_string(),
                            Some(SCROLL_PARTIAL_RETRY_HINT),
                            cx,
                        );
                    } else {
                        self.overlay_error = Some(error.to_string());
                    }
                }
                CaptureEffect::CheckScrollPermissions(ticket) => {
                    self.check_scroll_permissions(ticket, cx);
                }
                CaptureEffect::IdentifyScrollTarget(ticket) => {
                    self.identify_scroll_target(ticket, cx);
                }
                CaptureEffect::PrepareScrollWindows(ticket) => {
                    // Give the control window time to register before the exact
                    // own-window set is fixed and mapped.
                    self.pending_scroll_exclusion =
                        Some((ticket, Instant::now() + SCROLL_WINDOW_SETTLE));
                }
                CaptureEffect::ScrollPrepared(ticket) => {
                    self.scroll_exclusion = self.session.scroll_exclusion().unwrap_or(&[]).to_vec();
                    self.update_scroll_control_status(SCROLL_PREPARED_STATUS, None, cx);
                    log::info!(
                        "scroll_prepared ticket={ticket:?} exclusion={}",
                        self.scroll_exclusion.len()
                    );
                    // Input protection comes next: activate the target, then
                    // install the session tap, both bounded by a deadline.
                    self.pending_scroll_input = Some(PendingScrollInput {
                        ticket,
                        started: Instant::now(),
                    });
                }
                CaptureEffect::CaptureScrollFrame(ticket) => {
                    self.submit_scroll_frame(ticket, cx, ScrollFrameRoute::Initial);
                }
                CaptureEffect::VerifyScrollFrame(ticket) => {
                    self.verify_pending_scroll_frame(ticket, cx);
                }
                CaptureEffect::ScrollPreviewUpdated(ticket) => {
                    let _ = ticket;
                    self.refresh_scroll_preview(cx);
                }
                CaptureEffect::ScrollCollecting(ticket) => {
                    self.open_scroll_collection(ticket, cx);
                }
                CaptureEffect::ScrollRefused(ticket, refusal) => {
                    self.handle_scroll_refusal(ticket, refusal, cx);
                }
                CaptureEffect::ScrollPartialChoice(ticket, refusal) => {
                    self.handle_scroll_partial_choice(ticket, refusal, cx);
                }
                CaptureEffect::ScrollRelaunchRequired => {
                    self.close_scroll_ui(cx);
                    self.show_error(
                        "原生捕获状态无法确认。已禁止输出和再次截图，请退出并重新打开 ArcShot。"
                            .into(),
                        cx,
                    );
                }
            }
        }
        self.refresh_capture_view(cx);
        self.sync_preview(cx);
    }

    /// Keeps the composed selection pixels in step with the authoritative document.
    ///
    /// Only discrete edits or live drag updates change the key; composition runs
    /// on the background executor with at most one request in flight. A late
    /// result is applied only when the document still matches its snapshot.
    fn sync_preview(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.session.document() else {
            return;
        };
        let Ok(crop) = document.crop() else {
            return;
        };
        let key: PreviewKey = (crop, document.annotations().to_vec());
        if key.1.is_empty() {
            if self.preview.applied.take().is_some() {
                self.set_preview(None, cx);
            }
            return;
        }
        if self.preview.applied.as_ref() == Some(&key)
            || self.preview.in_flight.as_ref() == Some(&key)
        {
            return;
        }
        if self.preview.in_flight.is_some() {
            return;
        }
        let frame = document.frame().clone();
        let annotations = key.1.clone();
        self.preview.in_flight = Some(key.clone());
        let composed = cx
            .background_executor()
            .spawn(async move { arcshot_image::compose_document(&frame, crop, &annotations) });
        let entity = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = composed.await;
            entity.update(cx, |coordinator, cx| {
                coordinator.finish_preview(key, result, cx);
            });
        })
        .detach();
    }

    fn finish_preview(
        &mut self,
        key: PreviewKey,
        result: Result<arcshot_image::ComposedImage, arcshot_image::ComposeError>,
        cx: &mut Context<Self>,
    ) {
        if self.preview.in_flight.as_ref() == Some(&key) {
            self.preview.in_flight = None;
        }
        match result {
            Ok(image) => {
                let current = self.session.document().and_then(|document| {
                    document
                        .crop()
                        .ok()
                        .map(|crop| (crop, document.annotations().to_vec()))
                });
                if current.as_ref() == Some(&key) {
                    self.set_preview(Some(image), cx);
                    self.preview.applied = Some(key);
                }
            }
            Err(error) => log::warn!("preview_compose_failure={error}"),
        }
        self.sync_preview(cx);
    }

    /// Publishes composed RGBA pixels through the overlay's GPU image cache.
    fn set_preview(&mut self, image: Option<arcshot_image::ComposedImage>, cx: &mut Context<Self>) {
        let Some(handle) = self.overlay else {
            return;
        };
        let selection = self.session.document().and_then(Document::selection);
        let render = image.and_then(|image| {
            let bgra = match arcshot_image::preview_bgra(&image) {
                Ok(bgra) => bgra,
                Err(error) => {
                    log::warn!("preview_copy_failure={error}");
                    return None;
                }
            };
            let buffer = image::RgbaImage::from_raw(image.width(), image.height(), bgra)?;
            Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
        });
        let _ = handle.update(cx, |view, window, cx| {
            let previous = match (render, selection) {
                (Some(image), Some(selection)) => {
                    view.preview.replace(OverlayPreview { image, selection })
                }
                _ => view.preview.take(),
            };
            if let Some(previous) = previous {
                cx.drop_image(previous.image, Some(window));
            }
            cx.notify();
        });
    }

    /// Fences the document version, then performs the one target side effect.
    fn publish_output(
        &mut self,
        job: OutputJob,
        encoded: Result<Vec<u8>, OutputFailure>,
        cx: &mut Context<Self>,
    ) {
        if !self.session.may_publish(&job) {
            log::info!(
                "output_stale version={} encoded={}",
                job.version(),
                encoded.is_ok()
            );
            self.pending_save.clear();
            self.completion_preview_png = None;
            return;
        }
        // Only a confirmed clipboard copy may show the completion thumbnail;
        // its exact encoded bytes are reused instead of recomposing the image.
        self.completion_preview_png = None;
        let result = match encoded {
            Ok(png) => {
                log::info!(
                    "output_publish target={:?} bytes={} pixels={}x{} annotations={}",
                    job.target(),
                    png.len(),
                    job.crop().width(),
                    job.crop().height(),
                    job.annotations().len()
                );
                let outcome = arcshot_app::publish_output(
                    job.clone(),
                    &png,
                    &mut self.pending_save,
                    |png| {
                        let started = Instant::now();
                        let result = native::write_png_to_clipboard(png);
                        log::info!("clipboard_accept_us={}", started.elapsed().as_micros());
                        result.map_err(|error| {
                            log::error!("clipboard_failure={error}");
                            OutputFailure::Clipboard
                        })
                    },
                    |path, png| match native::write_png_to_file(path, png) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            log::error!("save_file_failure={error}");
                            Err(OutputFailure::SaveFile)
                        }
                    },
                );
                if outcome.is_ok() && job.target() == arcshot_core::OutputTarget::Clipboard {
                    self.completion_preview_png = Some(png);
                }
                outcome
            }
            Err(error) => {
                self.pending_save.clear();
                Err(error)
            }
        };
        self.action(CaptureAction::OutputCompleted { job, result }, cx);
        // The effect handler consumed the bytes when it showed the preview.
        self.completion_preview_png = None;
    }

    /// Applies the native save panel outcome and clears unused destinations.
    fn resolve_save_panel(&mut self, outcome: native::SavePanelOutcome, cx: &mut Context<Self>) {
        let Some(job) = self.pending_panel.take() else {
            log::warn!("save_panel_outcome_without_request");
            self.pending_save.clear();
            return;
        };
        let result = match outcome {
            native::SavePanelOutcome::Chosen(path) => {
                log::info!("save_panel_outcome=chosen version={}", job.version());
                self.pending_save.remember(job.clone(), path);
                SavePanelResult::Chosen
            }
            native::SavePanelOutcome::Cancelled => {
                log::info!("save_panel_outcome=cancelled version={}", job.version());
                SavePanelResult::Cancelled
            }
            native::SavePanelOutcome::Failed(error) => {
                log::error!("save_panel_failure={error}");
                SavePanelResult::Failed
            }
        };
        self.action(CaptureAction::SavePanelResolved { job, result }, cx);
        // Cancelled, failed or stale results leave no file destination behind.
        if !self.session.output_busy() {
            self.pending_save.clear();
        }
    }

    /// Releases the save panel and any chosen destination when the session ends.
    fn end_pending_save(&mut self) {
        if self.pending_panel.take().is_some() {
            native::cancel_save_panel();
            // An outcome that raced with the cancellation is already fenced off
            // by the dropped job; discard it instead of applying it to no panel.
            let _ = native::take_save_panel_outcome();
        }
        self.pending_save.clear();
    }

    fn refresh_capture_view(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.overlay else {
            return;
        };
        let document = self.session.document();
        let selection = document.and_then(Document::selection);
        let tool = document.map_or(Tool::Select, Document::tool);
        let selected = document.and_then(Document::selected_index);
        let annotations =
            document.map_or_else(Vec::new, |document| document.annotations().to_vec());
        let rectangle_style =
            document.map_or_else(RectangleStyle::default, Document::rectangle_style);
        let line_style = document.map_or_else(LineStyle::default, Document::line_style);
        let arrow_style = document.map_or_else(ArrowStyle::default, Document::arrow_style);
        let blur_style = document.map_or_else(BlurStyle::default, Document::blur_style);
        let text_style = document.map_or_else(TextStyle::default, Document::text_style);
        let text_edit = document
            .and_then(|document| document.text_edit())
            .map(|edit| TextEditMirror {
                anchor: edit.anchor(),
                content: edit.content().to_owned(),
                bounds: edit.bounds(),
                style: edit.style(),
                target: edit.target(),
            });
        let density = self.session.density();
        let busy = self.session.output_busy();
        let error = self.overlay_error.take();
        let updated = handle.update(cx, |view, window, cx| {
            view.selection = selection;
            view.tool = tool;
            view.selected = selected;
            view.annotations = annotations;
            view.rectangle_style = rectangle_style;
            view.line_style = line_style;
            view.arrow_style = arrow_style;
            view.blur_style = blur_style;
            view.text_style = text_style;
            view.text_edit = text_edit;
            view.sync_text_editor(window, cx);
            view.density = density;
            view.busy = busy;
            view.error = error;
            cx.notify();
        });
        if updated.is_err() {
            self.overlay = None;
        }
    }

    fn submit(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        log::info!(
            "capture_submit_us={}",
            self.started
                .map(|s| s.elapsed().as_micros())
                .unwrap_or_default()
        );
        // All own visible windows are fixed before submission. No other UI is created
        // until the result is resolved, and the status item cannot reopen a native menu.
        cx.global_mut::<MenuBarGlobal>().0.hide_for_capture();
        let native_prep_started = Instant::now();
        let prepared = native::builtin_display()
            .and_then(|display| native::owned_visible_window_ids().map(|ids| (display, ids)));
        log::info!(
            "capture_native_prep_us={}",
            native_prep_started.elapsed().as_micros()
        );
        let (display, ids) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                cx.global_mut::<MenuBarGlobal>().0.restore_after_capture();
                log::error!("capture_prepare_failure={error}");
                let failure = classify_native_failure(&error);
                self.action(
                    CaptureAction::Completed {
                        ticket,
                        result: Err(failure),
                    },
                    cx,
                );
                return;
            }
        };
        let submitted = self.tasks.submit(ticket, Instant::now(), move || {
            let worker_started = Instant::now();
            let result = native::capture_builtin(display, ids).map_err(|error| {
                log::error!("capture_failure={error}");
                classify_native_failure(&error)
            })?;
            log::info!("capture_worker_us={}", worker_started.elapsed().as_micros());
            log::info!(
                "capture_request_us={} display={} pixels={}x{} stride={} scale={}",
                result.request_duration.as_micros(),
                result.display.id,
                result.width,
                result.height,
                result.stride,
                result.display.scale
            );
            let screen = geometry(result.display).map_err(|_| CaptureFailure::InvalidFrame)?;
            let frame = CapturedFrame::try_from_srgb_bgra(
                screen,
                result.width,
                result.height,
                result.stride,
                result.bgra,
            )
            .map_err(|error| match error {
                FrameError::Budget => CaptureFailure::ResourceLimit,
                _ => CaptureFailure::InvalidFrame,
            })?;
            log::info!("{}", capture_probe(&frame));
            Ok(frame)
        });
        if let Err(error) = submitted {
            cx.global_mut::<MenuBarGlobal>().0.restore_after_capture();
            self.action(
                CaptureAction::Completed {
                    ticket,
                    result: Err(match error {
                        CaptureSubmitError::WorkerUnavailable => CaptureFailure::WorkerUnavailable,
                        _ => CaptureFailure::NativeUncertain,
                    }),
                },
                cx,
            );
        }
    }

    fn prepare_preview(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        if !self.wake_reported {
            log::info!(
                "capture_prepare_submit_us={}",
                self.started
                    .map(|s| s.elapsed().as_micros())
                    .unwrap_or_default()
            );
        }
        let Some(frame) = self.session.frame().cloned() else {
            return;
        };
        let screen = frame.geometry();
        self.region_geometry = Some(screen);
        let preview = cx.background_executor().spawn(async move {
            let prepare_started = Instant::now();
            let preview = arcshot_image::capture_preview(&frame).map_err(|e| e.to_string())?;
            let buffer = image::RgbaImage::from_raw(preview.width, preview.height, preview.bgra)
                .ok_or_else(|| "invalid preview layout".to_owned())?;
            let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
            log::info!(
                "capture_prepare_us={} pixels={}x{}",
                prepare_started.elapsed().as_micros(),
                preview.width,
                preview.height
            );
            Ok::<_, String>(image)
        });
        self.preview_task = Some(cx.spawn(async move |entity, cx| {
            let preview = preview.await;
            let _ = entity.update(cx, |state, cx| {
                // Fence immediately before window creation, not just frame admission.
                if !state.session.may_present(ticket) {
                    return;
                }
                if !native::builtin_display()
                    .ok()
                    .and_then(|d| geometry(d).ok())
                    .is_some_and(|d| d == screen)
                {
                    state.action(CaptureAction::DisplayInvalidated, cx);
                    return;
                }
                match preview {
                    Ok(image) => {
                        state.close_overlay(cx);
                        let open_started = Instant::now();
                        match state.open_frozen(ticket, screen, image, cx) {
                            Ok(()) => {
                                log::info!(
                                    "capture_open_window_us={}",
                                    open_started.elapsed().as_micros()
                                );
                                // A re-presented frozen overlay (after a Scroll
                                // cancel/refusal) starts with no view state;
                                // mirror the authoritative document so the
                                // pre-scroll selection and its buttons return.
                                state.refresh_capture_view(cx);
                                if !state.wake_reported {
                                    state.wake_reported = true;
                                    log::info!(
                                        "capture_wake_window_submit_us={} pixels={:?}",
                                        state
                                            .started
                                            .map(|s| s.elapsed().as_micros())
                                            .unwrap_or_default(),
                                        screen.pixel_size()
                                    );
                                }
                            }
                            Err(error) => {
                                state.action(CaptureAction::Cancel, cx);
                                state.show_error(format!("无法显示冻结画面：{error}"), cx);
                            }
                        }
                    }
                    Err(error) => {
                        state.action(CaptureAction::Cancel, cx);
                        state.show_error(format!("无法准备冻结预览：{error}"), cx);
                    }
                }
            });
        }));
    }

    fn close_overlay(&mut self, cx: &mut Context<Self>) {
        self.overlay_activation_pending = false;
        self.region_geometry = None;
        self.preview = PreviewSync::default();
        if let Some(handle) = self.overlay.take() {
            let _ = handle.update(cx, |view, window, cx| {
                if let Some(image) = view.image.take() {
                    cx.drop_image(image, Some(window));
                }
                if let Some(preview) = view.preview.take() {
                    cx.drop_image(preview.image, Some(window));
                }
                window.remove_window();
            });
        }
    }
    fn close_notice(&mut self, cx: &mut Context<Self>) {
        self.settings_notice = false;
        if let Some(handle) = self.notice.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// Reads the real three-permission Scroll preflight and advances the session.
    fn check_scroll_permissions(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        // The control UI is the only owned window allowed to stay visible during
        // preparation; it joins the exclusion set fixed later.
        self.open_scroll_control(ticket, cx);
        let available = native::scroll_permissions();
        self.log_scroll_permissions(available);
        // The fixed preview window must exist before the own-window exclusion
        // set is mapped, so it opens during preparation, never mid-collection.
        if available.all_available() {
            self.open_scroll_preview(cx);
        }
        self.action(
            CaptureAction::ScrollPermissionsChecked { ticket, available },
            cx,
        );
        // The notice flow mirrors the same real result for the user-facing window.
        // The initial check only shows a notice when something is missing; a
        // fully available initial check proceeds silently to target identification.
        let _ = self
            .scroll_permissions
            .dispatch(ScrollPermissionAction::Checked(available));
        if matches!(
            self.scroll_permissions.notice(),
            Some(ScrollPermissionNotice::Missing(_))
        ) {
            self.sync_scroll_notice(cx);
        } else {
            // A fully available initial check never opens a notice; clear the
            // flow's Ready state too so it cannot outlive this preparation.
            let _ = self
                .scroll_permissions
                .dispatch(ScrollPermissionAction::Cancel);
            self.close_scroll_notice(cx);
        }
    }

    /// Logs the preflight without attributing a TCC denial reason.
    fn log_scroll_permissions(&self, available: arcshot_core::ScrollPermissions) {
        let missing = available.missing();
        let missing = if missing.is_empty() {
            "none".to_owned()
        } else {
            missing
                .iter()
                .map(|permission| permission.key())
                .collect::<Vec<_>>()
                .join(",")
        };
        log::info!(
            "scroll_permission screen_recording={} input_monitoring={} accessibility={} missing={}",
            available.screen_recording,
            available.input_monitoring,
            available.accessibility,
            missing
        );
    }

    /// Submits the bounded target probe; no AppKit object crosses the worker.
    ///
    /// The target does not need to be frontmost yet: the user just used
    /// ArcShot's overlay, so ArcShot legitimately is. The foreground gate runs
    /// after preparation, when ArcShot activates the locked target.
    fn identify_scroll_target(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        let Some(selection) = self.session.scroll_selection() else {
            return;
        };
        let Ok(display) = native::builtin_display() else {
            self.action(CaptureAction::DisplayInvalidated, cx);
            return;
        };
        let Some(pointer) = native::pointer_location_cg(display) else {
            self.action(CaptureAction::ScrollNativeUncertain { ticket }, cx);
            return;
        };
        let submitted = self
            .scroll_target_tasks
            .submit(ticket, Instant::now(), move || {
                native::probe_scroll_target(selection, display, pointer)
            });
        if submitted.is_err() {
            // A busy gate or an unstartable worker leaves native state unconfirmed.
            self.action(CaptureAction::ScrollNativeUncertain { ticket }, cx);
        }
    }

    /// Handles one known-state Scroll refusal: keep the region session, no output.
    ///
    /// Target and window refusals restore the frozen overlay immediately and
    /// surface the reason inside it. A permission refusal shows the notice first
    /// and restores the overlay when that notice closes, so the app never exposes
    /// two windows at once.
    fn handle_scroll_refusal(
        &mut self,
        ticket: CaptureTicket,
        refusal: ScrollRefusal,
        cx: &mut Context<Self>,
    ) {
        log::info!("scroll_refused reason={}", scroll_refusal_reason(&refusal));
        log::info!(
            "scroll_partial_choice available=false reason={}",
            scroll_refusal_reason(&refusal)
        );
        self.close_scroll_control(cx);
        self.close_scroll_preview(cx);
        self.scroll_stitch = None;
        self.scroll_raw_queue.clear();
        match &refusal {
            ScrollRefusal::Permissions(_) => {
                self.scroll_restore = Some(ticket);
                self.sync_scroll_notice(cx);
            }
            _ => {
                self.overlay_error = Some(scroll_refusal_message(&refusal).to_owned());
            }
        }
    }

    /// Handles one known-state stop with a verified result (I-11 step 11).
    ///
    /// The control window (and the verified preview) stay; input protection
    /// retires in `tick`. The user must explicitly use the verified partial
    /// (incomplete, copied only after confirmation) or abandon it. Nothing
    /// is output here.
    fn handle_scroll_partial_choice(
        &mut self,
        _ticket: CaptureTicket,
        refusal: ScrollRefusal,
        cx: &mut Context<Self>,
    ) {
        log::info!("scroll_refused reason={}", scroll_refusal_reason(&refusal));
        let size = self
            .session
            .scroll_stitch_frame()
            .map_or([0, 0], |frame| frame.geometry().pixel_size());
        log::info!(
            "scroll_partial_choice available=true reason={} pixels={}x{}",
            scroll_refusal_reason(&refusal),
            size[0],
            size[1]
        );
        // Unverified queued frames are discarded; verified history is kept.
        self.scroll_raw_queue.clear();
        self.update_scroll_control_partial(
            scroll_refusal_message(&refusal),
            Some(PARTIAL_INCOMPLETE_WARNING),
            cx,
        );
    }

    /// Drives the Scroll permission notice policy and its native effects.
    fn scroll_permission(&mut self, action: ScrollPermissionAction, cx: &mut Context<Self>) {
        if matches!(action, ScrollPermissionAction::Cancel) {
            self.scroll_visiting_settings = false;
            self.scroll_settings_departed = false;
            // A permission refusal deferred the region overlay; the notice
            // closing is the point where the selection becomes visible again.
            if let Some(ticket) = self.scroll_restore.take() {
                self.prepare_preview(ticket, cx);
            }
        }
        let effects = self.scroll_permissions.dispatch(action);
        let mut open_settings = None;
        for effect in effects {
            match effect {
                ScrollPermissionEffect::Preflight => {
                    let available = native::scroll_permissions();
                    self.log_scroll_permissions(available);
                    let _ = self
                        .scroll_permissions
                        .dispatch(ScrollPermissionAction::Checked(available));
                }
                ScrollPermissionEffect::OpenSettings(permission) => {
                    open_settings = Some(permission);
                }
                ScrollPermissionEffect::ShowNotice(_) => {}
            }
        }
        self.sync_scroll_notice(cx);
        if let Some(permission) = open_settings {
            // Open after constructing our notice, so we do not steal focus back
            // from System Settings and mistake that for a user return.
            match native::open_scroll_permission_settings(permission) {
                Ok(()) => {
                    self.scroll_visiting_settings = true;
                    self.scroll_settings_departed = false;
                }
                Err(error) => self.show_error(format!("无法打开系统设置：{error}"), cx),
            }
        }
    }

    /// Opens, updates or closes the Scroll permission notice from the flow state.
    fn sync_scroll_notice(&mut self, cx: &mut Context<Self>) {
        let notice = self.scroll_permissions.notice().cloned();
        match notice {
            Some(notice) => {
                let updated = self.scroll_notice.as_ref().and_then(|handle| {
                    handle
                        .update(cx, |view, _, cx| {
                            view.notice = notice.clone();
                            cx.notify();
                        })
                        .ok()
                });
                if updated.is_none() {
                    self.open_scroll_notice(notice, cx);
                }
            }
            None => self.close_scroll_notice(cx),
        }
    }

    fn open_scroll_notice(&mut self, notice: ScrollPermissionNotice, cx: &mut Context<Self>) {
        let queue = Rc::clone(&self.queue);
        let bounds = WindowBounds::centered(size(px(560.0), px(300.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let close_queue = Rc::clone(&queue);
                let window_id = gpui::Window::window_handle(window).window_id();
                window.on_window_should_close(cx, move |_, _| {
                    close_queue.borrow_mut().push(UiCommand::ScrollPermission(
                        window_id,
                        ScrollPermissionAction::Cancel,
                    ));
                    true
                });
                let focus = cx.focus_handle();
                cx.new(|_| ScrollPermissionView {
                    notice,
                    focus: focus.clone(),
                    queue,
                })
            },
        );
        match result {
            Ok(handle) => {
                self.scroll_notice = Some(handle);
                cx.activate(true);
            }
            Err(error) => log::error!("scroll_notice_window_failure={error}"),
        }
    }

    fn close_scroll_notice(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.scroll_notice.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// Opens the minimal control UI that stays visible during preparation.
    fn open_scroll_control(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        if self.scroll_control.is_some() {
            return;
        }
        self.scroll_control_id.borrow_mut().take();
        let queue = Rc::clone(&self.queue);
        let control_id = Rc::clone(&self.scroll_control_id);
        // Tall enough for the partial-result choice row (two labelled
        // buttons plus status and hint) as well as the collecting status.
        let bounds = WindowBounds::centered(size(px(380.0), px(180.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                kind: WindowKind::PopUp,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                // Read the exact public window number from the live NSView. The
                // control is the only own window the event tap may pass input
                // to, even while a dismissed overlay is still leaving the
                // WindowServer asynchronously.
                let number = raw_window_handle::HasWindowHandle::window_handle(window)
                    .ok()
                    .and_then(|handle| match handle.as_raw() {
                        raw_window_handle::RawWindowHandle::AppKit(handle) => {
                            native::ns_window_number(handle.ns_view.as_ptr())
                        }
                        _ => None,
                    });
                *control_id.borrow_mut() = number;
                log::info!("scroll_control_window number={number:?}");
                let close_queue = Rc::clone(&queue);
                let window_id = gpui::Window::window_handle(window).window_id();
                window.on_window_should_close(cx, move |_, _| {
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::ScrollControlClosed(window_id, ticket));
                    true
                });
                let focus = cx.focus_handle();
                cx.new(|_| ScrollControlView {
                    status: SCROLL_PREPARING_STATUS.to_owned(),
                    hint: None,
                    partial_choice: false,
                    ticket,
                    focus: focus.clone(),
                    queue,
                })
            },
        );
        match result {
            Ok(handle) => {
                self.scroll_control = Some(handle);
                cx.activate(true);
            }
            Err(error) => log::error!("scroll_control_window_failure={error}"),
        }
    }

    fn close_scroll_control(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.scroll_control.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    /// Opens the one fixed live preview window during preparation.
    ///
    /// Placement prefers the right of the selection, then the left, otherwise
    /// the selection's inner bottom-right (I-11 §5). The window set is fixed
    /// here; collection only updates the image it presents.
    fn open_scroll_preview(&mut self, cx: &mut Context<Self>) {
        if self.scroll_preview.is_some() {
            return;
        }
        self.scroll_preview_id.borrow_mut().take();
        let Some(selection) = self.session.scroll_selection() else {
            return;
        };
        let Ok(display) = native::builtin_display() else {
            log::error!("scroll_preview_display_unavailable");
            return;
        };
        let placement = scroll_preview_placement(selection, &display);
        let preview_id = Rc::clone(&self.scroll_preview_id);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(placement.bounds)),
                kind: WindowKind::PopUp,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                // The preview is non-interactive: mouse events must reach the
                // target underneath, while accessibility activation is kept.
                let ns_view = raw_window_handle::HasWindowHandle::window_handle(window)
                    .ok()
                    .and_then(|handle| match handle.as_raw() {
                        raw_window_handle::RawWindowHandle::AppKit(handle) => {
                            Some(handle.ns_view.as_ptr().cast::<core::ffi::c_void>())
                        }
                        _ => None,
                    });
                let click_through = ns_view
                    .map(native::set_scroll_preview_click_through)
                    .unwrap_or(false);
                let number = ns_view.and_then(native::ns_window_number);
                *preview_id.borrow_mut() = number;
                log::info!("scroll_preview_window number={number:?} click_through={click_through}");
                cx.new(|_| ScrollPreviewView { image: None })
            },
        );
        match result {
            Ok(handle) => {
                self.scroll_preview = Some(handle);
                self.scroll_preview_place = placement.place;
                log::info!(
                    "scroll_preview placed={} frame=[{:.0},{:.0},{:.0},{:.0}]",
                    placement.place,
                    f32::from(placement.bounds.origin.x),
                    f32::from(placement.bounds.origin.y),
                    f32::from(placement.bounds.size.width),
                    f32::from(placement.bounds.size.height)
                );
            }
            Err(error) => log::error!("scroll_preview_window_failure={error}"),
        }
    }

    fn close_scroll_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.scroll_preview.take() {
            let _ = handle.update(cx, |view, window, cx| {
                view.set_image(None, window);
                cx.notify();
                window.remove_window();
            });
        }
    }

    /// Opens the fixed completion thumbnail at the bottom-right (I-11 §7).
    ///
    /// The window presents the exact PNG that was copied, proportionally scaled
    /// and letterboxed. It slides in, stays for the documented three seconds,
    /// then slides out and fades; hover never extends the stay.
    fn open_completion_preview(
        &mut self,
        ticket: CaptureTicket,
        png: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.close_completion_preview(cx);
        let display = native::builtin_display()?;
        let mut image = image::load_from_memory(&png)
            .map_err(|error| anyhow::anyhow!("completion preview decode: {error}"))?
            .to_rgba8();
        // The decoder yields RGBA; GPUI's `RenderImage` reads BGRA, the same
        // layout every other preview surface hands it.
        arcshot_image::rgba_to_bgra_in_place(&mut image);
        let (pixels_width, pixels_height) = (image.width(), image.height());
        // V1 captures are fixed at 2x, so the logical image is half the pixels.
        const CAPTURE_SCALE: f64 = 2.0;
        let layout = preview_layout(
            [
                f64::from(pixels_width) / CAPTURE_SCALE,
                f64::from(pixels_height) / CAPTURE_SCALE,
            ],
            [display.width, display.height],
        );
        let render = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
        let bounds = Bounds {
            origin: Point::new(px(layout.x as f32), px(layout.y as f32)),
            size: size(px(layout.width as f32), px(layout.height as f32)),
        };
        let queue = Rc::clone(&self.queue);
        let handle = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::PopUp,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let close_queue = Rc::clone(&queue);
                window.on_window_should_close(cx, move |_, _| {
                    // Closing the thumbnail is equivalent to activating it: both
                    // enter the shared annotation surface for this attempt.
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::EnterCompletionAnnotation(ticket));
                    true
                });
                cx.new(|_| CompletionPreviewView {
                    image: Some(render),
                    ticket,
                    queue,
                    width: layout.width as f32,
                })
            },
        )?;
        self.completion_preview = Some(handle);
        self.completion_preview_deadline = Some((
            ticket,
            Instant::now()
                + COMPLETION_PREVIEW_ENTER
                + COMPLETION_PREVIEW_HOLD
                + COMPLETION_PREVIEW_EXIT,
        ));
        log::info!(
            "completion_preview shown ticket={ticket:?} frame=[{:.0},{:.0},{:.0},{:.0}] pixels={pixels_width}x{pixels_height}",
            layout.x,
            layout.y,
            layout.width,
            layout.height
        );
        // The thumbnail never takes keyboard focus: the target stays frontmost
        // and the user chooses it with the pointer.
        Ok(())
    }

    /// Closes the completion thumbnail and cancels its pending auto-dismiss.
    fn close_completion_preview(&mut self, cx: &mut Context<Self>) {
        self.completion_preview_deadline = None;
        if let Some(handle) = self.completion_preview.take() {
            let _ = handle.update(cx, |view, window, cx| {
                if let Some(image) = view.image.take() {
                    cx.drop_image(image, Some(window));
                }
                window.remove_window();
            });
        }
    }

    /// Presents the confirmed whole-image document in the shared editor.
    ///
    /// The window stays screen-sized; the complete long image is fitted inside
    /// it and the view transform pans/zooms without touching the document crop
    /// or annotation coordinates, so the output extent never changes (I-03/I-11 §8).
    fn open_completion_annotation(
        &mut self,
        ticket: CaptureTicket,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.close_completion_preview(cx);
        if !self.session.may_present(ticket) {
            return Err(anyhow::anyhow!("completion annotation fence failed"));
        }
        let Some(frame) = self.session.frame().cloned() else {
            return Err(anyhow::anyhow!("completion document is gone"));
        };
        let screen = frame.geometry();
        let preview = arcshot_image::capture_preview(&frame)?;
        let buffer = image::RgbaImage::from_raw(preview.width, preview.height, preview.bgra)
            .ok_or_else(|| anyhow::anyhow!("invalid completion preview layout"))?;
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
        let [_, _, width, height] = screen.source_rect();
        let display = native::builtin_display()?;
        let viewport = [display.width, display.height];
        let zoom = (viewport[0] / width).min(viewport[1] / height).min(1.0);
        let offset = [
            (viewport[0] - width * zoom) / 2.0,
            (viewport[1] - height * zoom) / 2.0,
        ];
        let display_id = cx
            .displays()
            .into_iter()
            .find(|candidate| u64::from(candidate.id()) == u64::from(screen.display_id()))
            .ok_or_else(|| anyhow::anyhow!("built-in display absent from GPUI"))?
            .id();
        let queue = Rc::clone(&self.queue);
        self.overlay_error = None;
        // A stale region overlay must not survive into the annotation surface;
        // the original display anchor stays for display-loss detection.
        if let Some(handle) = self.overlay.take() {
            let _ = handle.update(cx, |view, window, cx| {
                if let Some(image) = view.image.take() {
                    cx.drop_image(image, Some(window));
                }
                if let Some(preview) = view.preview.take() {
                    cx.drop_image(preview.image, Some(window));
                }
                window.remove_window();
            });
        }
        self.overlay = Some(cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(viewport[0] as f32), px(viewport[1] as f32)),
                })),
                display_id: Some(display_id),
                kind: WindowKind::PopUp,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let close_queue = Rc::clone(&queue);
                window.on_window_should_close(cx, move |_, _| {
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::CancelCapture(ticket));
                    true
                });
                let focus = cx.focus_handle();
                let entity = cx.new(|_| CaptureView {
                    ticket,
                    image: Some(image),
                    selection: None,
                    busy: false,
                    error: None,
                    screen: Some(screen),
                    drag: None,
                    tool: Tool::default(),
                    selected: None,
                    annotations: Vec::new(),
                    rectangle_style: RectangleStyle::default(),
                    line_style: LineStyle::default(),
                    arrow_style: ArrowStyle::default(),
                    blur_style: BlurStyle::default(),
                    text_style: TextStyle::default(),
                    text_edit: None,
                    text_input: None,
                    text_focus: None,
                    size_editing: false,
                    size_draft: String::new(),
                    preview: None,
                    hovered_tool: None,
                    density: Density::Compact,
                    long_image: true,
                    view_zoom: zoom,
                    view_offset: offset,
                    viewport,
                    focus: focus.clone(),
                    queue,
                });
                window.focus(&focus, cx);
                entity
            },
        )?);
        self.overlay_activation_pending = true;
        cx.activate(true);
        log::info!(
            "completion_annotation_opened ticket={ticket:?} document={width}x{height} zoom={zoom:.3}"
        );
        self.refresh_capture_view(cx);
        self.sync_preview(cx);
        Ok(())
    }

    /// Closes every Scroll-owned window; the region session is untouched.
    fn close_scroll_ui(&mut self, cx: &mut Context<Self>) {
        self.close_scroll_control(cx);
        self.close_scroll_preview(cx);
        self.close_scroll_notice(cx);
    }

    /// Drives one deferred input-protection installation step (I-12).
    ///
    /// The locked target is brought to the foreground cooperatively; once it
    /// owns the foreground the session tap is installed with the control
    /// window as its only own-UI pass-through. Every failure is a known-state
    /// input refusal: no output, the region session is kept.
    fn drive_scroll_input(&mut self, pending: PendingScrollInput, cx: &mut Context<Self>) {
        let ticket = pending.ticket;
        let deadline_reached = pending.started.elapsed() >= SCROLL_ACTIVATE_DEADLINE;
        if self.session.scroll_phase() != ScrollPhase::Prepared {
            return; // Preparation moved on (cancel, refusal); nothing to install.
        }
        let Some(target) = self.session.scroll_target().cloned() else {
            return;
        };
        if deadline_reached {
            log::info!("scroll_input_failure reason=NotFrontmost deadline=true");
            self.action(
                CaptureAction::ScrollInputInstalled {
                    ticket,
                    result: Err(ScrollInputError::NotFrontmost),
                },
                cx,
            );
            return;
        }
        match native::activate_scroll_target(target.owner_pid) {
            Ok(()) => self.install_scroll_tap(pending, cx),
            // Cooperative activation can lag one run-loop turn; keep retrying
            // until the absolute deadline.
            Err(ScrollTargetError::NotFrontmost) => {
                self.pending_scroll_input = Some(pending);
            }
            // The target process died between lock and activation.
            Err(error) => {
                log::info!("scroll_input_failure reason={error:?}");
                self.action(CaptureAction::ScrollTargetLost { ticket, error }, cx);
            }
        }
    }

    /// Installs the session tap once the target owns the foreground.
    fn install_scroll_tap(&mut self, pending: PendingScrollInput, cx: &mut Context<Self>) {
        let ticket = pending.ticket;
        let refuse_input =
            |coordinator: &mut Self, error: ScrollInputError, cx: &mut Context<Self>| {
                log::info!("scroll_input_failure reason={error:?}");
                coordinator.action(
                    CaptureAction::ScrollInputInstalled {
                        ticket,
                        result: Err(error),
                    },
                    cx,
                );
            };
        if self.scroll_tap.is_some() {
            refuse_input(self, ScrollInputError::StillDraining, cx);
            return;
        }
        // No UI window may appear or disappear between the fixed exclusion set
        // and collection: no new id may appear, and the dismissed overlay must
        // be gone so only the control window remains visible. A closing window
        // can take a few run-loop turns to leave the WindowServer, so wait under
        // the same absolute deadline instead of starting with a mixed set.
        let ids = match native::owned_visible_window_ids() {
            Ok(ids) => ids,
            Err(error) => {
                log::error!("scroll_input_window_failure={error}");
                refuse_input(self, ScrollInputError::TapUnavailable, cx);
                return;
            }
        };
        if ids.iter().any(|id| !self.scroll_exclusion.contains(id)) {
            log::error!(
                "scroll_input_window_set_changed fixed={} live={}",
                self.scroll_exclusion.len(),
                ids.len()
            );
            refuse_input(self, ScrollInputError::TapUnavailable, cx);
            return;
        }
        // A window from the preparation-time set (for example the dismissed
        // region overlay) may still be leaving the WindowServer. That is not a
        // reason to fail; wait until exactly the control + fixed preview set
        // remains. An unknown live window was already refused above.
        let Some(control_id) = *self.scroll_control_id.borrow() else {
            log::error!("scroll_control_window_missing");
            refuse_input(self, ScrollInputError::TapUnavailable, cx);
            return;
        };
        let Some(preview_id) = *self.scroll_preview_id.borrow() else {
            log::error!("scroll_preview_window_missing");
            refuse_input(self, ScrollInputError::TapUnavailable, cx);
            return;
        };
        if ids.len() != 2 || !ids.contains(&control_id) || !ids.contains(&preview_id) {
            // A dismissed region overlay can take a few run-loop turns to
            // leave the WindowServer. Wait under the same absolute deadline
            // for exactly the control + fixed preview set; an unknown window
            // is already refused above.
            log::info!(
                "scroll_input_waiting_for_window_settle live={} control={control_id} preview={preview_id}",
                ids.len()
            );
            self.pending_scroll_input = Some(pending);
            return;
        }
        // These are the exclusion arrays used for every collected frame, fixed
        // here immediately before collection: the control UI plus the fixed
        // live preview, never a window created mid-collection.
        self.scroll_capture_exclusion = ids.clone();
        log::info!(
            "scroll_exclusion_fixed prepared={} collecting={} control={control_id} preview={preview_id}",
            self.scroll_exclusion.len(),
            ids.len()
        );
        let control = match native::owned_visible_window_frames(&ids) {
            Ok(frames) => match frames.iter().find(|(id, _)| *id == control_id) {
                Some((_, frame)) => *frame,
                None => {
                    log::error!("scroll_input_owned_windows={}", frames.len());
                    refuse_input(self, ScrollInputError::TapUnavailable, cx);
                    return;
                }
            },
            Err(error) => {
                log::error!("scroll_input_frame_failure={error:?}");
                refuse_input(self, ScrollInputError::TapUnavailable, cx);
                return;
            }
        };
        match native::ScrollInputTap::install() {
            Ok(tap) => {
                tap.set_control_rect(control);
                self.scroll_tap = Some((ticket, tap));
                // No menus or popovers can be opened behind the tap; the item
                // is restored the moment Scroll becomes inactive again.
                cx.global::<MenuBarGlobal>().0.hide_for_capture();
                log::info!("scroll_input installed=true owned_windows={}", ids.len());
                self.action(
                    CaptureAction::ScrollInputInstalled {
                        ticket,
                        result: Ok(()),
                    },
                    cx,
                );
            }
            Err(native::ScrollTapError::GestureInProgress) => {
                // The physical press that activated the Scroll entry may still
                // be held for a few run-loop turns; retry under the same
                // absolute deadline instead of starting mid-gesture.
                self.pending_scroll_input = Some(pending);
            }
            Err(_) => refuse_input(self, ScrollInputError::TapUnavailable, cx),
        }
    }

    /// Submits one fixed-crop native frame request under the single gate.
    ///
    /// The route decides whether a pre-request failure stops preparation or
    /// stops an open collection; the capture, admission and probe are shared.
    fn submit_scroll_frame(
        &mut self,
        ticket: CaptureTicket,
        cx: &mut Context<Self>,
        route: ScrollFrameRoute,
    ) {
        let failure = |coordinator: &mut Self, failure: CaptureFailure, cx: &mut Context<Self>| {
            let action = match route {
                ScrollFrameRoute::Initial => CaptureAction::ScrollFrameAdmitted {
                    ticket,
                    result: Err(failure),
                },
                ScrollFrameRoute::Collected => CaptureAction::ScrollFrameCollected {
                    ticket,
                    result: Err(failure),
                },
            };
            coordinator.action(action, cx);
        };
        let Some(document) = self.session.document() else {
            return;
        };
        let Some(selection) = self.session.scroll_selection() else {
            return;
        };
        let region = document.frame().geometry();
        let Ok(crop) = selection.pixel_rect(region) else {
            log::error!("scroll_frame_crop_invalid");
            failure(self, CaptureFailure::InvalidFrame, cx);
            return;
        };
        let Ok(display) = native::builtin_display() else {
            failure(self, CaptureFailure::DisplayInvalid, cx);
            return;
        };
        let exclusion = self.scroll_capture_exclusion.clone();
        self.scroll_frame_submitted = Some(Instant::now());
        let submitted = self
            .scroll_frame_tasks
            .submit(ticket, Instant::now(), move || {
                let result = native::capture_scroll_region(
                    display,
                    exclusion,
                    (crop.x(), crop.y(), crop.width(), crop.height()),
                )
                .map_err(|error| {
                    log::error!("scroll_frame_failure={error}");
                    classify_native_failure(&error)
                })?;
                log::info!(
                    "scroll_frame_request_us={} pixels={}x{} stride={} scale={}",
                    result.request_duration.as_micros(),
                    result.width,
                    result.height,
                    result.stride,
                    result.display.scale
                );
                let frame = arcshot_app::admit_scroll_capture(
                    region,
                    crop,
                    result.width,
                    result.height,
                    result.stride,
                    result.bgra,
                )
                .inspect_err(|error| log::error!("scroll_frame_failure={error}"))?;
                log::info!("{}", capture_probe(&frame));
                Ok(frame)
            });
        match submitted {
            Ok(()) => {}
            Err(ScrollFrameSubmitError::Busy) => {
                // A previous request still owns the gate; its own result fence
                // will resolve this preparation or refuse it.
                log::warn!("scroll_frame_submit_busy");
            }
            Err(ScrollFrameSubmitError::RelaunchRequired) => {
                failure(self, CaptureFailure::NativeUncertain, cx);
            }
            Err(ScrollFrameSubmitError::WorkerUnavailable) => {
                failure(self, CaptureFailure::WorkerUnavailable, cx);
            }
        }
    }

    /// Routes one finished native request into the bounded queue or the
    /// authoritative collector.
    fn handle_scroll_frame_result(
        &mut self,
        ticket: CaptureTicket,
        result: Result<CapturedFrame, CaptureFailure>,
        cx: &mut Context<Self>,
    ) {
        let collecting = self.session.scroll_phase() == ScrollPhase::Collecting
            && !self.session.output_busy()
            && self
                .scroll_tap
                .as_ref()
                .is_some_and(|(active, _)| *active == ticket);
        if !collecting {
            // Late results are fenced by the session version/ticket; they are
            // discarded with a diagnostic, never queued into a new session.
            log::info!("scroll_frame_stale ticket={ticket:?}");
            return;
        }
        match result {
            Ok(frame) => {
                if let Err(error) = self.scroll_raw_queue.push(frame) {
                    log::error!("scroll_frame_backpressure={error}");
                    self.action(
                        CaptureAction::ScrollFrameCollected {
                            ticket,
                            result: Err(CaptureFailure::ResourceLimit),
                        },
                        cx,
                    );
                }
            }
            Err(failure) => {
                self.action(
                    CaptureAction::ScrollFrameCollected {
                        ticket,
                        result: Err(failure),
                    },
                    cx,
                );
            }
        }
        self.drain_scroll_raw_queue(cx);
    }

    /// Verifies one queued raw frame at a time; stopping clears the rest.
    fn drain_scroll_raw_queue(&mut self, cx: &mut Context<Self>) {
        while self.session.scroll_phase() == ScrollPhase::Collecting && !self.session.output_busy()
        {
            let Some(ticket) = self.scroll_tap.as_ref().map(|(ticket, _)| *ticket) else {
                break;
            };
            let Some(frame) = self.scroll_raw_queue.pop() else {
                break;
            };
            self.action(
                CaptureAction::ScrollFrameCollected {
                    ticket,
                    result: Ok(frame),
                },
                cx,
            );
        }
        if self.session.scroll_phase() != ScrollPhase::Collecting || self.session.output_busy() {
            self.scroll_raw_queue.clear();
        }
    }

    /// Submits another request only while the gate is idle and the bounded
    /// raw queue is not full (I-13 backpressure).
    fn pump_scroll_frames(&mut self, cx: &mut Context<Self>) {
        if self.session.scroll_phase() != ScrollPhase::Collecting || self.session.output_busy() {
            return;
        }
        // A deferred restart is waiting for the slot: do not queue another
        // request behind it, or the restart could never observe a free gate.
        if self.restart_gate.is_pending() {
            return;
        }
        let Some(ticket) = self.scroll_tap.as_ref().map(|(ticket, _)| *ticket) else {
            return;
        };
        if may_submit_scroll_frame(self.scroll_frame_tasks.busy(), self.scroll_raw_queue.len()) {
            self.submit_scroll_frame(ticket, cx, ScrollFrameRoute::Collected);
        }
    }

    /// Runs the strict image verifier on the held raw frame and reports the
    /// decision to the authoritative session.
    fn verify_pending_scroll_frame(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        let Some(frame) = self.session.take_scroll_pending_frame() else {
            return;
        };
        let stitch_started = Instant::now();
        let result = match self.scroll_stitch.as_mut() {
            Some(builder) => builder.submit(&frame),
            None => {
                log::error!("scroll_stitch_missing");
                self.action(
                    CaptureAction::ScrollStitchRefused {
                        ticket,
                        error: ScrollStitchError::Geometry,
                    },
                    cx,
                );
                return;
            }
        };
        log::info!(
            "scroll_stitch_verify_us={}",
            stitch_started.elapsed().as_micros()
        );
        if let Some(submitted) = self.scroll_frame_submitted.take() {
            let finished = Instant::now();
            log::info!("frame_verify_us={}", submitted.elapsed().as_micros());
            self.scroll_frame_latency = Some((submitted, finished));
        }
        match result {
            Ok(arcshot_image::StitchDecision::Extended {
                placement,
                added_rows,
            }) => {
                let (top, bottom, extensions, duplicates) =
                    self.scroll_stitch.as_ref().map_or((0, 0, 0, 0), |builder| {
                        (
                            builder.top(),
                            builder.bottom(),
                            builder.extensions(),
                            builder.duplicates(),
                        )
                    });
                log::info!(
                    "scroll_stitch extended offset={} added_rows={} top={} bottom={} extensions={} duplicates={}",
                    placement.offset,
                    added_rows,
                    top,
                    bottom,
                    extensions,
                    duplicates
                );
                self.report_verified(ticket, true, cx);
            }
            Ok(arcshot_image::StitchDecision::Duplicate { placement }) => {
                let (top, bottom, extensions, duplicates) =
                    self.scroll_stitch.as_ref().map_or((0, 0, 0, 0), |builder| {
                        (
                            builder.top(),
                            builder.bottom(),
                            builder.extensions(),
                            builder.duplicates(),
                        )
                    });
                log::info!(
                    "scroll_stitch duplicate offset={} top={} bottom={} extensions={} duplicates={}",
                    placement.offset,
                    top,
                    bottom,
                    extensions,
                    duplicates
                );
                self.report_verified(ticket, false, cx);
            }
            Err(refusal) => {
                log::info!("scroll_stitch_refused reason={refusal}");
                if let Some(span) = self
                    .scroll_stitch
                    .as_ref()
                    .and_then(arcshot_image::StitchBuilder::last_mismatch_span)
                {
                    log::info!(
                        "scroll_stitch_mismatch rows={}..{} cols={}..{}",
                        span.first_row,
                        span.last_row,
                        span.first_col,
                        span.last_col
                    );
                }
                self.action(
                    CaptureAction::ScrollStitchRefused {
                        ticket,
                        error: map_stitch_refusal(refusal),
                    },
                    cx,
                );
            }
        }
    }

    /// Hands a verified stitched document to the session.
    fn report_verified(&mut self, ticket: CaptureTicket, extended: bool, cx: &mut Context<Self>) {
        let verified = self
            .scroll_stitch
            .as_ref()
            .and_then(|builder| builder.current_frame().ok());
        match verified {
            Some(frame) => self.action(
                CaptureAction::ScrollStitchVerified {
                    ticket,
                    frame,
                    extended,
                },
                cx,
            ),
            None => self.action(
                CaptureAction::ScrollStitchRefused {
                    ticket,
                    error: ScrollStitchError::ResourceLimit,
                },
                cx,
            ),
        }
    }

    /// Refreshes the fixed preview window in place from verified history.
    fn refresh_scroll_preview(&mut self, cx: &mut Context<Self>) {
        if self.session.scroll_phase() != ScrollPhase::Collecting {
            return;
        }
        let Some(builder) = self.scroll_stitch.as_ref() else {
            return;
        };
        let Some(preview) = builder.preview(SCROLL_PREVIEW_MAX_PIXELS) else {
            return;
        };
        let Some(handle) = self.scroll_preview else {
            return;
        };
        if self
            .scroll_preview_last
            .is_some_and(|last| last.elapsed() < SCROLL_PREVIEW_THROTTLE)
        {
            return;
        }
        self.scroll_preview_last = Some(Instant::now());
        // Hand the packed bytes to the image buffer directly: cloning them only
        // doubles the live preview surface for every verified frame.
        let image = image::RgbaImage::from_raw(preview.width, preview.height, preview.bgra)
            .map(|buffer| Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])));
        let _ = handle.update(cx, |view, window, cx| {
            view.set_image(image, window);
            cx.notify();
        });
        log::info!(
            "scroll_preview_updated place={} pixels={}x{} document={}x{} extensions={} duplicates={}",
            self.scroll_preview_place,
            preview.width,
            preview.height,
            builder.width(),
            builder.height(),
            builder.extensions(),
            builder.duplicates()
        );
        if let Some((submitted, finished)) = self.scroll_frame_latency.take() {
            log::info!(
                "frame_preview_us={} preview_update_us={}",
                submitted.elapsed().as_micros(),
                finished.elapsed().as_micros()
            );
        }
        if let Some(started) = self.scroll_event_started.take() {
            // Remember the reported event: reading `last_scroll` again without
            // this would re-time the same event at every later preview update.
            self.scroll_event_reported = Some(started);
            log::info!("event_preview_us={}", started.elapsed().as_micros());
        }
    }

    /// Clears the per-session latency bookkeeping (PERF-* diagnostics).
    ///
    /// These instants belong to one collection: a session that ends before a
    /// preview update would otherwise report the next session's first preview
    /// against a timestamp from the previous one, inflating the observed
    /// input-to-preview latency by the whole gap between sessions.
    fn reset_scroll_latency(&mut self) {
        self.scroll_started = None;
        self.scroll_frame_submitted = None;
        self.scroll_frame_latency = None;
        self.scroll_event_started = None;
        self.scroll_event_reported = None;
    }

    /// Opens collection: raw scroll inside the fixed selection is released,
    /// the strict verifier starts at the trusted origin, and the control UI
    /// states the concise operation hint (I-11 §5).
    fn open_scroll_collection(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        match self.session.scroll_frame().cloned() {
            Some(origin) => match arcshot_image::StitchBuilder::new(&origin) {
                Ok(builder) => self.scroll_stitch = Some(builder),
                Err(error) => {
                    log::error!("scroll_stitch_origin_failure={error}");
                    self.action(
                        CaptureAction::ScrollStitchRefused {
                            ticket,
                            error: map_stitch_refusal(error),
                        },
                        cx,
                    );
                    return;
                }
            },
            None => {
                log::error!("scroll_stitch_origin_missing");
                self.action(
                    CaptureAction::ScrollStitchRefused {
                        ticket,
                        error: ScrollStitchError::Geometry,
                    },
                    cx,
                );
                return;
            }
        }
        if let Some(selection) = self.session.scroll_selection()
            && let Some(display) = native::builtin_display().ok()
            && let Some((_, tap)) = self.scroll_tap.as_ref()
        {
            tap.open_collection(native::GlobalRect {
                x: display.x + selection.x(),
                y: display.y + selection.y(),
                width: selection.width(),
                height: selection.height(),
            });
        }
        self.scroll_target_checked = Some(Instant::now());
        self.update_scroll_control_status(
            SCROLL_COLLECTING_STATUS,
            Some(SCROLL_COLLECTING_HINT),
            cx,
        );
        log::info!("scroll_collecting ticket={ticket:?}");
        if let Some(started) = self.scroll_started.take() {
            log::info!("scroll_start_us={}", started.elapsed().as_micros());
        }
        // The origin is already verified: show it immediately, then start the
        // bounded request loop (at most one in-flight request and two queued).
        self.scroll_preview_last = None;
        self.refresh_scroll_preview(cx);
        self.pump_scroll_frames(cx);
    }

    /// Drains and retires the session tap once Scroll is inactive.
    ///
    /// The tap first switches to release-only interception so no owned press
    /// leaks its release to the target; once drained it is dropped, its native
    /// sources are removed and its counters are logged.
    fn retire_scroll_tap(&mut self) {
        let Some((ticket, tap)) = self.scroll_tap.take() else {
            return;
        };
        tap.begin_drain();
        // I-12: never drop the tap while it still owns a press, or that press's
        // release would reach the target. A tap the system disabled cannot
        // intercept anything, so dropping it cannot leak a release either.
        if tap.drained() || tap.protection_lost() {
            let stats = tap.stats();
            log::info!(
                "scroll_input_retired ticket={ticket:?} scrolls={} clicks_blocked={} keys_owned={} protection_lost={}",
                stats.scrolls_passed,
                stats.clicks_blocked,
                stats.keys_owned,
                tap.protection_lost()
            );
            self.reset_scroll_latency();
            // `tap` drops here: the run-loop source is removed and the port
            // invalidated before the gate's memory is released.
        } else {
            // Still releasing owned input; retry on a later tick.
            self.scroll_tap = Some((ticket, tap));
        }
    }

    fn update_scroll_control_status(
        &mut self,
        status: &str,
        hint: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let updated = self.scroll_control.as_ref().and_then(|handle| {
            handle
                .update(cx, |view, _, cx| {
                    view.status = status.to_owned();
                    view.hint = hint.map(str::to_owned);
                    view.partial_choice = false;
                    cx.notify();
                })
                .ok()
        });
        if updated.is_none() {
            self.scroll_control = None;
        }
    }

    /// Shows the explicit partial-result choices in the control window.
    fn update_scroll_control_partial(
        &mut self,
        status: &str,
        hint: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let updated = self.scroll_control.as_ref().and_then(|handle| {
            handle
                .update(cx, |view, _, cx| {
                    view.status = status.to_owned();
                    view.hint = hint.map(str::to_owned);
                    view.partial_choice = true;
                    cx.notify();
                })
                .ok()
        });
        if updated.is_none() {
            self.scroll_control = None;
        }
    }

    fn open_preparing(
        &mut self,
        ticket: CaptureTicket,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Option<u32>> {
        self.close_overlay(cx);
        let queue = Rc::clone(&self.queue);
        let density = self.session.density();
        let bounds = WindowBounds::centered(size(px(420.0), px(110.0)), cx);
        let number = Rc::new(RefCell::new(None));
        let number_for_closure = Rc::clone(&number);
        self.overlay = Some(cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                kind: WindowKind::PopUp,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                *number_for_closure.borrow_mut() =
                    raw_window_handle::HasWindowHandle::window_handle(window)
                        .ok()
                        .and_then(|handle| match handle.as_raw() {
                            raw_window_handle::RawWindowHandle::AppKit(handle) => {
                                native::ns_window_number(handle.ns_view.as_ptr())
                            }
                            _ => None,
                        });
                let close_queue = Rc::clone(&queue);
                window.on_window_should_close(cx, move |_, _| {
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::CancelCapture(ticket));
                    true
                });
                let focus = cx.focus_handle();
                let entity = cx.new(|_| CaptureView {
                    ticket,
                    image: None,
                    selection: None,
                    busy: false,
                    error: None,
                    screen: None,
                    drag: None,
                    tool: Tool::default(),
                    selected: None,
                    annotations: Vec::new(),
                    rectangle_style: RectangleStyle::default(),
                    line_style: LineStyle::default(),
                    arrow_style: ArrowStyle::default(),
                    blur_style: BlurStyle::default(),
                    text_style: TextStyle::default(),
                    text_edit: None,
                    text_input: None,
                    text_focus: None,
                    size_editing: false,
                    size_draft: String::new(),
                    preview: None,
                    hovered_tool: None,
                    density,
                    long_image: false,
                    view_zoom: 1.0,
                    view_offset: [0.0, 0.0],
                    viewport: [0.0, 0.0],
                    focus: focus.clone(),
                    queue,
                });
                window.focus(&focus, cx);
                entity
            },
        )?);
        cx.activate(true);
        Ok(*number.borrow())
    }

    fn open_frozen(
        &mut self,
        ticket: CaptureTicket,
        screen: ScreenGeometry,
        image: Arc<RenderImage>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.overlay_error = None;
        let [_, _, w, h] = screen.source_rect();
        let display_id = cx
            .displays()
            .into_iter()
            .find(|display| u64::from(display.id()) == u64::from(screen.display_id()))
            .ok_or_else(|| anyhow::anyhow!("built-in display absent from GPUI"))?
            .id();
        let queue = Rc::clone(&self.queue);
        let density = self.session.density();
        self.overlay = Some(cx.open_window(
            WindowOptions {
                // GPUI macOS window bounds are display-local, unlike ScreenGeometry's
                // global origin. Display identity is explicit; no main-screen fallback.
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(w as f32), px(h as f32)),
                })),
                display_id: Some(display_id),
                kind: WindowKind::PopUp,
                titlebar: None,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let close_queue = Rc::clone(&queue);
                window.on_window_should_close(cx, move |_, _| {
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::CancelCapture(ticket));
                    true
                });
                let focus = cx.focus_handle();
                let entity = cx.new(|_| CaptureView {
                    ticket,
                    image: Some(image),
                    selection: None,
                    busy: false,
                    error: None,
                    screen: Some(screen),
                    drag: None,
                    tool: Tool::default(),
                    selected: None,
                    annotations: Vec::new(),
                    rectangle_style: RectangleStyle::default(),
                    line_style: LineStyle::default(),
                    arrow_style: ArrowStyle::default(),
                    blur_style: BlurStyle::default(),
                    text_style: TextStyle::default(),
                    text_edit: None,
                    text_input: None,
                    text_focus: None,
                    size_editing: false,
                    size_draft: String::new(),
                    preview: None,
                    hovered_tool: None,
                    density,
                    long_image: false,
                    view_zoom: 1.0,
                    view_offset: [0.0, 0.0],
                    viewport: [w, h],
                    focus: focus.clone(),
                    queue,
                });
                window.focus(&focus, cx);
                entity
            },
        )?);
        self.overlay_activation_pending = true;
        cx.activate(true);
        Ok(())
    }

    fn permission(&mut self, action: PermissionAction, cx: &mut Context<Self>) {
        if matches!(action, PermissionAction::Cancel | PermissionAction::Later) {
            self.visiting_settings = false;
            self.settings_departed = false;
        }
        let mut effects: VecDeque<_> = self.permissions.dispatch(action).into();
        let mut open_settings = false;
        while let Some(effect) = effects.pop_front() {
            match effect {
                PermissionEffect::RememberGuide => native::mark_permission_guide_seen(),
                PermissionEffect::Preflight => effects.extend(self.permissions.dispatch(
                    PermissionAction::Checked(native::screen_recording_available()),
                )),
                PermissionEffect::RequestScreenRecording => {
                    // Request outcome is not availability; only the later preflight is.
                    let _ = native::request_screen_recording();
                }
                PermissionEffect::OpenSettings => open_settings = true,
            }
        }
        self.close_notice(cx);
        if self.permissions.notice().is_some() {
            self.open_notice(None, CAPTURE_ERROR_HEADING, cx);
        }
        if open_settings {
            // Open after constructing our notice, so we do not steal focus back
            // from System Settings and mistake that for a user return.
            match native::open_screen_recording_settings() {
                Ok(()) => {
                    self.visiting_settings = true;
                    self.settings_departed = false;
                }
                Err(error) => self.show_error(format!("无法打开系统设置：{error}"), cx),
            }
        }
    }
    fn show_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.close_notice(cx);
        self.open_notice(Some(message), CAPTURE_ERROR_HEADING, cx);
    }
    /// Reports a menu-preference failure; dismissal leaves the menu available.
    fn show_preferences_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.close_notice(cx);
        self.open_notice(Some(message), PREFERENCE_ERROR_HEADING, cx);
        self.settings_notice = true;
    }
    fn open_notice(
        &mut self,
        error: Option<String>,
        error_heading: &'static str,
        cx: &mut Context<Self>,
    ) {
        let notice = if error.is_some() {
            None
        } else {
            self.permissions.notice()
        };
        let queue = Rc::clone(&self.queue);
        let bounds = WindowBounds::centered(size(px(560.0), px(250.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(bounds),
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let close_queue = Rc::clone(&queue);
                let window_id = gpui::Window::window_handle(window).window_id();
                window.on_window_should_close(cx, move |_, _| {
                    close_queue
                        .borrow_mut()
                        .push(UiCommand::Permission(window_id, PermissionAction::Cancel));
                    true
                });
                let focus = cx.focus_handle();
                let entity = cx.new(|_| NoticeView {
                    notice,
                    error,
                    error_heading,
                    focus: focus.clone(),
                    queue,
                });
                window.focus(&focus, cx);
                entity
            },
        );
        match result {
            Ok(handle) => {
                self.notice = Some(handle);
                cx.activate(true);
            }
            Err(error) => log::error!("notice_window_failure={error}"),
        }
    }
}
/// User-facing text for each preference failure; the failure itself is logged too.
fn preferences_notice_text(notice: PreferencesNotice) -> String {
    match notice {
        PreferencesNotice::LoginItemToggleFailed => {
            "“登录时启动”没能切换成功。已重新读取系统里的实际状态，可再次切换重试。".to_owned()
        }
        PreferencesNotice::LoginItemUnreadable => {
            "无法读取“登录时启动”的实际状态，本次未做任何更改；可再次切换重试。".to_owned()
        }
        PreferencesNotice::LoginItemApprovalRequired => {
            "已在系统登录项中登记，但还需要在“系统设置 › 通用 › 登录项”中允许；当前不会随登录启动。"
                .to_owned()
        }
        PreferencesNotice::DensityUnreadable => {
            "无法读取“选项密度”的当前设置，本次未做任何更改；可再次切换重试。".to_owned()
        }
        PreferencesNotice::DensityWriteFailed => {
            "“选项密度”没能保存成功。已重新读取实际设置，当前显示的是真实值；可再次切换重试。"
                .to_owned()
        }
    }
}

fn format_style(style: AnnotationStyle) -> String {
    match style {
        AnnotationStyle::Rectangle(style) => format!(
            "stroke={} width={} fill={}",
            style.stroke().hex(),
            style.width().label(),
            style.fill()
        ),
        AnnotationStyle::Line(style) => format!(
            "stroke={} width={} dash={}",
            style.stroke().hex(),
            style.width().label(),
            style.dash()
        ),
        AnnotationStyle::Arrow(style) => format!(
            "stroke={} width={} inward={}",
            style.stroke().hex(),
            style.width().label(),
            style.inward()
        ),
        AnnotationStyle::Blur(style) => format!("level={}", style.level().label()),
        AnnotationStyle::Text(style) => {
            format!("stroke={} size={}", style.color().hex(), style.size_label())
        }
    }
}

/// Content-free diagnostic label for one Scroll refusal; no window titles.
fn scroll_refusal_reason(refusal: &ScrollRefusal) -> String {
    match refusal {
        ScrollRefusal::NoSelection => "NoSelection".to_owned(),
        ScrollRefusal::AnnotationsPresent => "AnnotationsPresent".to_owned(),
        ScrollRefusal::Permissions(missing) => format!(
            "Permissions missing={}",
            missing
                .iter()
                .map(|permission| permission.key())
                .collect::<Vec<_>>()
                .join(",")
        ),
        ScrollRefusal::Target(error) => format!("Target {error:?}"),
        ScrollRefusal::Windows(error) => format!("Windows {error:?}"),
        ScrollRefusal::Input(error) => format!("Input {error:?}"),
        ScrollRefusal::Frame(error) => format!("Frame {error:?}"),
        ScrollRefusal::Stitch(error) => format!("Stitch {error:?}"),
    }
}

/// User-facing text for a known-state Scroll refusal; permissions use the notice.
fn scroll_refusal_message(refusal: &ScrollRefusal) -> &'static str {
    match refusal {
        ScrollRefusal::NoSelection => "没有有效选区，无法进入滚动截图。",
        ScrollRefusal::AnnotationsPresent => {
            "当前选区已有标注。请先撤销或删除全部标注，再进入滚动截图。"
        }
        ScrollRefusal::Permissions(_) => "滚动截图权限尚不可用。",
        ScrollRefusal::Target(ScrollTargetError::NoTarget) => {
            "选区不在任何可识别的目标窗口内，无法开始滚动截图。"
        }
        ScrollRefusal::Target(ScrollTargetError::AmbiguousTarget) => {
            "选区跨越多个窗口，无法确定唯一目标。"
        }
        ScrollRefusal::Target(ScrollTargetError::NotFrontmost) => {
            "目标应用未在前台，请先切换到目标窗口。"
        }
        ScrollRefusal::Target(ScrollTargetError::PointerMismatch) => {
            "指针下的窗口与目标不一致，请将指针移到目标窗口内。"
        }
        ScrollRefusal::Target(ScrollTargetError::Geometry) => "目标几何无法确认。",
        ScrollRefusal::Target(ScrollTargetError::NativeUncertain) => {
            "无法确认目标窗口的公开状态，未产生输出。"
        }
        ScrollRefusal::Windows(ScrollWindowError::MappingFailed) => {
            "无法排除 ArcShot 自有窗口，未开始滚动截图。"
        }
        ScrollRefusal::Windows(ScrollWindowError::NativeUncertain)
        | ScrollRefusal::Windows(ScrollWindowError::NotMainThread) => {
            "无法确认自有窗口的公开状态，未产生输出。"
        }
        ScrollRefusal::Input(ScrollInputError::NotFrontmost) => {
            "目标应用未能切换到前台，已停止；可重试。"
        }
        ScrollRefusal::Input(ScrollInputError::TapUnavailable) => {
            "无法安装输入保护，未开始采集；可重试。"
        }
        ScrollRefusal::Input(ScrollInputError::GestureInProgress) => {
            "请先松开鼠标按键与回车/Esc 键，再进入滚动截图。"
        }
        ScrollRefusal::Input(ScrollInputError::ProtectionLost) => {
            "输入保护中断，已停止采集；未产生输出。"
        }
        ScrollRefusal::Input(ScrollInputError::StillDraining) => {
            "上一次滚动截图的输入尚未完全释放，请稍后重试。"
        }
        ScrollRefusal::Frame(arcshot_core::ScrollFrameError::Admission) => {
            "首帧未通过像素校验，未产生输出；可重试。"
        }
        ScrollRefusal::Frame(arcshot_core::ScrollFrameError::Geometry) => {
            "首帧几何与选区不一致，未产生输出；可重试。"
        }
        ScrollRefusal::Frame(arcshot_core::ScrollFrameError::ResourceLimit) => {
            "选区超出滚动截图的尺寸/内存预算，未产生输出。"
        }
        ScrollRefusal::Frame(arcshot_core::ScrollFrameError::Worker) => {
            "无法启动滚动截图工作线程，未产生输出；可重试。"
        }
        ScrollRefusal::Stitch(ScrollStitchError::Geometry) => {
            "拼接帧几何与选区不一致，已停止采集；未产生输出。"
        }
        ScrollRefusal::Stitch(ScrollStitchError::Ambiguous) => {
            "滚动位移无法唯一确认，已停止采集；未产生输出。"
        }
        ScrollRefusal::Stitch(ScrollStitchError::ContentChanged) => {
            "已采集内容在重叠区域发生变化，无法可靠拼接；未产生输出。"
        }
        ScrollRefusal::Stitch(ScrollStitchError::Gap) => {
            "滚动出现无法验证的缺口，已停止采集；未产生输出。"
        }
        ScrollRefusal::Stitch(ScrollStitchError::ResourceLimit) => {
            "滚动拼接超出尺寸/内存预算，已停止采集；未产生输出。"
        }
    }
}

/// The fixed preview window's resolved placement and bounds.
struct ScrollPreviewPlacement {
    bounds: Bounds<gpui::Pixels>,
    place: &'static str,
}

/// Resolves the three documented preview positions against the selection and
/// the built-in display. Right is preferred, then left, otherwise the
/// selection's inner bottom-right; the result is clamped on screen.
fn scroll_preview_placement(
    selection: Selection,
    display: &DisplaySnapshot,
) -> ScrollPreviewPlacement {
    let width = SCROLL_PREVIEW_WIDTH;
    let height = SCROLL_PREVIEW_HEIGHT;
    let right_x = selection.x() + selection.width() + SCROLL_PREVIEW_MARGIN;
    let left_x = selection.x() - SCROLL_PREVIEW_MARGIN - width;
    let (x, place) = if right_x + width <= display.width {
        (right_x, "right")
    } else if left_x >= 0.0 {
        (left_x, "left")
    } else {
        (selection.x() + selection.width() - width, "inner")
    };
    let mut y = if place == "inner" {
        selection.y() + selection.height() - height
    } else {
        selection.y()
    };
    if y + height > display.height {
        y = display.height - height;
    }
    let y = y.max(0.0);
    let x = x.max(0.0);
    ScrollPreviewPlacement {
        bounds: Bounds {
            origin: Point::new(px(x as f32), px(y as f32)),
            size: size(px(width as f32), px(height as f32)),
        },
        place,
    }
}

/// Maps the image verifier's refusal onto the authoritative semantic error.
fn map_stitch_refusal(refusal: arcshot_image::StitchRefusal) -> ScrollStitchError {
    match refusal {
        arcshot_image::StitchRefusal::Geometry => ScrollStitchError::Geometry,
        arcshot_image::StitchRefusal::Ambiguous => ScrollStitchError::Ambiguous,
        arcshot_image::StitchRefusal::ContentChanged => ScrollStitchError::ContentChanged,
        arcshot_image::StitchRefusal::Gap => ScrollStitchError::Gap,
        arcshot_image::StitchRefusal::ResourceLimit => ScrollStitchError::ResourceLimit,
    }
}

fn classify_native_failure(error: &native::CaptureError) -> CaptureFailure {
    match error {
        native::CaptureError::Display => CaptureFailure::DisplayInvalid,
        native::CaptureError::Budget => CaptureFailure::ResourceLimit,
        _ if error.requires_relaunch() => CaptureFailure::NativeUncertain,
        _ => CaptureFailure::InvalidFrame,
    }
}

fn geometry(display: DisplaySnapshot) -> Result<ScreenGeometry, FrameError> {
    ScreenGeometry::try_new(
        display.id,
        display.x,
        display.y,
        display.width,
        display.height,
        display.scale,
    )
}

/// The tool whose style an option action changes, if any.
const fn action_style_tool(action: &CaptureAction) -> Option<Tool> {
    match action {
        CaptureAction::RectangleOption { .. } => Some(Tool::Rectangle),
        CaptureAction::LineOption { .. } => Some(Tool::Line),
        CaptureAction::ArrowOption { .. } => Some(Tool::Arrow),
        CaptureAction::BlurOption { .. } => Some(Tool::Blur),
        CaptureAction::TextOption { .. } => Some(Tool::Text),
        CaptureAction::SelectTool { tool, .. } => Some(*tool),
        _ => None,
    }
}

/// Diagnostic label for one semantic action; frame payloads are never printed.
const fn action_name(action: &CaptureAction) -> &'static str {
    match action {
        CaptureAction::Start => "Start",
        CaptureAction::PermissionChecked { .. } => "PermissionChecked",
        CaptureAction::Completed { .. } => "Completed",
        CaptureAction::Cancel => "Cancel",
        CaptureAction::CancelAttempt(_) => "CancelAttempt",
        CaptureAction::DisplayInvalidated => "DisplayInvalidated",
        CaptureAction::Save { .. } => "Save",
        CaptureAction::SavePanelResolved { .. } => "SavePanelResolved",
        CaptureAction::SelectRegion { .. } => "SelectRegion",
        CaptureAction::PointerPressed { .. } => "PointerPressed",
        CaptureAction::PointerMoved { .. } => "PointerMoved",
        CaptureAction::PointerReleased { .. } => "PointerReleased",
        CaptureAction::SelectTool { .. } => "SelectTool",
        CaptureAction::RectangleOption { .. } => "RectangleOption",
        CaptureAction::LineOption { .. } => "LineOption",
        CaptureAction::ArrowOption { .. } => "ArrowOption",
        CaptureAction::BlurOption { .. } => "BlurOption",
        CaptureAction::TextOption { .. } => "TextOption",
        CaptureAction::TextBegin { .. } => "TextBegin",
        CaptureAction::TextEdited { .. } => "TextEdited",
        CaptureAction::TextCommit { .. } => "TextCommit",
        CaptureAction::TextCancel { .. } => "TextCancel",
        CaptureAction::DeleteSelected { .. } => "DeleteSelected",
        CaptureAction::DeselectSelected { .. } => "DeselectSelected",
        CaptureAction::Undo { .. } => "Undo",
        CaptureAction::Redo { .. } => "Redo",
        CaptureAction::Finish { .. } => "Finish",
        CaptureAction::FinishAt { .. } => "FinishAt",
        CaptureAction::OutputCompleted { .. } => "OutputCompleted",
        CaptureAction::EnterScroll { .. } => "EnterScroll",
        CaptureAction::ScrollPermissionsChecked { .. } => "ScrollPermissionsChecked",
        CaptureAction::ScrollTargetIdentified { .. } => "ScrollTargetIdentified",
        CaptureAction::ScrollWindowsPrepared { .. } => "ScrollWindowsPrepared",
        CaptureAction::ScrollInputInstalled { .. } => "ScrollInputInstalled",
        CaptureAction::ScrollFrameAdmitted { .. } => "ScrollFrameAdmitted",
        CaptureAction::ScrollFrameCollected { .. } => "ScrollFrameCollected",
        CaptureAction::ScrollStitchVerified { .. } => "ScrollStitchVerified",
        CaptureAction::ScrollStitchRefused { .. } => "ScrollStitchRefused",
        CaptureAction::ScrollInputLost { .. } => "ScrollInputLost",
        CaptureAction::ScrollTargetLost { .. } => "ScrollTargetLost",
        CaptureAction::CompleteScroll { .. } => "CompleteScroll",
        CaptureAction::EnterCompletionAnnotation { .. } => "EnterCompletionAnnotation",
        CaptureAction::AddDefaultRectangle { .. } => "AddDefaultRectangle",
        CaptureAction::CancelScroll { .. } => "CancelScroll",
        CaptureAction::AcceptScrollPartial { .. } => "AcceptScrollPartial",
        CaptureAction::AbandonScrollPartial { .. } => "AbandonScrollPartial",
        CaptureAction::ScrollNativeUncertain { .. } => "ScrollNativeUncertain",
    }
}

/// Text-only 5x5 grid probe of an admitted frame for the manual color oracle.
/// Rows are y = 5/25/50/75/95% of height; columns x = 5/25/50/75/95% of width.
/// The inset first/last rows avoid the menu bar and Dock strips that any
/// menu-triggered capture necessarily includes. BGRA storage is reported as
/// RRGGBBAA hex; values only, never pixels.
fn capture_probe(frame: &CapturedFrame) -> String {
    let [w, h] = frame.geometry().pixel_size().map(|v| v as usize);
    let stride = frame.stride();
    let px = |x: usize, y: usize| -> String {
        let i = y * stride + x * 4;
        let p = &frame.bgra()[i..i + 4];
        format!("{:02x}{:02x}{:02x}{:02x}", p[2], p[1], p[0], p[3])
    };
    let rows = [0.05, 0.25, 0.50, 0.75, 0.95].map(|f| {
        let y = (h as f64 * f) as usize;
        [0.05, 0.25, 0.50, 0.75, 0.95]
            .map(|g| px((w as f64 * g) as usize, y))
            .join(" ")
    });
    format!("capture_probe_grid rows={}", rows.join(" | "))
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    fn frame(bytes: Vec<u8>, stride: usize) -> CapturedFrame {
        CapturedFrame::try_from_srgb_bgra(
            ScreenGeometry::try_new(1, 0.0, 0.0, 1.0, 1.0, 2.0).unwrap(),
            2,
            2,
            stride,
            bytes,
        )
        .unwrap()
    }

    #[test]
    fn probe_reports_rgb_grid_and_ignores_padding() {
        // BGRA storage, row stride 12 with 4 padding bytes per row.
        let bytes = vec![
            0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0xff, 0xEE, 0xEE, 0xEE, 0xEE, //
            0xff, 0x00, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0xEE, 0xEE, 0xEE, 0xEE,
        ];
        let report = capture_probe(&frame(bytes, 12));
        let row0 = "ff0000ff ff0000ff 000000ff 000000ff 000000ff";
        let row1 = "0000ffff 0000ffff 00ff00ff 00ff00ff 00ff00ff";
        assert_eq!(
            report,
            format!("capture_probe_grid rows={row0} | {row0} | {row1} | {row1} | {row1}")
        );
    }
}
