//! Connects the authoritative session to safe native operations and GPUI windows.
use crate::{
    MenuBarGlobal,
    ui::{CaptureView, HotkeyView, NoticeView, OverlayPreview, UiCommand, UiQueue},
};
use arcshot_app::{
    CaptureSubmitError, CaptureTasks, GuideHistory, HotkeyAction, HotkeyEffect, HotkeyFlow,
    HotkeyNotice, HotkeyOutcome, LoginItemOutcome, OutputSubmitError, OutputTasks, PendingSave,
    PermissionAction, PermissionEffect, PermissionFlow, PreferencesAction, PreferencesEffect,
    PreferencesFlow, PreferencesNotice, encode_document,
};
use arcshot_core::{
    Annotation, AnnotationStyle, ArrowStyle, BlurStyle, CaptureAction, CaptureEffect,
    CaptureFailure, CapturePhase, CaptureSession, CaptureTicket, CapturedFrame, Document,
    FrameError, LineStyle, OutputFailure, OutputJob, PixelRect, RectangleStyle, SavePanelResult,
    ScreenGeometry, Tool,
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
/// Heading of a capture failure notice; it is not a settings problem.
const CAPTURE_ERROR_HEADING: &str = "无法继续截图";
/// Heading of a menu-preference failure notice.
const PREFERENCE_ERROR_HEADING: &str = "无法更新登录项设置";

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
    pending: Option<(CaptureTicket, Instant)>,
    /// True while the frozen overlay still needs one active-application check.
    overlay_activation_pending: bool,
    preview_task: Option<Task<()>>,
    started: Option<Instant>,
    settings_departed: bool,
    visiting_settings: bool,
    preview: PreviewSync,
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
    log::info!(
        "hotkey_launch config={} adapter={:?}",
        hotkey_flow.config(),
        hotkeys.is_some()
    );
    // The persisted menu preference is the only authority for a silent success.
    log::info!("preferences_launch completion_sound={completion_sound}");
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
        preferences: PreferencesFlow::new(completion_sound),
        preferences_error: None,
        settings_notice: false,
        queue: Rc::new(RefCell::new(Vec::new())),
        overlay: None,
        overlay_error: None,
        notice: None,
        pending: None,
        overlay_activation_pending: false,
        preview_task: None,
        started: None,
        settings_departed: false,
        visiting_settings: false,
        preview: PreviewSync::default(),
    });
    coordinator.update(cx, |state, cx| {
        state
            .session
            .set_completion_sound(state.preferences.completion_sound());
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
                    self.action(CaptureAction::Cancel, cx);
                    self.permission(PermissionAction::Check, cx);
                }
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
        if let Some((ticket, ready)) = self.pending
            && Instant::now() >= ready
        {
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
        if let Some((ticket, result)) = self.tasks.poll(Instant::now()) {
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
        // No SCK call is made for this continuous screen validity check.
        if let Some(frame) = self.session.frame() {
            let expected = frame.geometry();
            if !native::builtin_display()
                .ok()
                .and_then(|d| geometry(d).ok())
                .is_some_and(|d| d == expected)
            {
                self.action(CaptureAction::DisplayInvalidated, cx);
            }
        }
    }

    /// The shared entry for the menu command and the global hotkey.
    fn start_capture(&mut self, cx: &mut Context<Self>) {
        self.permission(PermissionAction::Cancel, cx);
        self.started = Some(Instant::now());
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
                let window_id = window.window_handle().window_id();
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
                | CaptureAction::DeleteSelected { .. }
                | CaptureAction::DeselectSelected { .. }
        );
        let before = self.session.phase();
        let effects: VecDeque<_> = self.session.dispatch(action).into();
        let after = self.session.phase();
        if after != before {
            log::info!("capture_phase from={before:?} to={after:?} action={name}");
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
        effects
    }

    fn action(&mut self, action: CaptureAction, cx: &mut Context<Self>) {
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
                    if let Err(error) = self.open_preparing(ticket, cx) {
                        self.show_error(format!("无法打开截图准备窗口：{error}"), cx);
                        effects.extend(self.dispatch(CaptureAction::Completed {
                            ticket,
                            result: Err(CaptureFailure::WorkerUnavailable),
                        }));
                    } else {
                        self.pending = Some((ticket, Instant::now() + PREPARE_SETTLE));
                    }
                }
                CaptureEffect::PresentFrozen(ticket) => self.prepare_preview(ticket, cx),
                CaptureEffect::DismissOverlay => {
                    self.preview_task = None;
                    self.end_pending_save();
                    self.close_overlay(cx);
                }
                CaptureEffect::PermissionUnavailable => {
                    self.permission(PermissionAction::Unavailable, cx)
                }
                CaptureEffect::ReportFailure(error) => {
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
                    let submitted = self
                        .output_tasks
                        .submit(fence, move || encode_document(&frame, snapshot));
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
                CaptureEffect::ReportOutputFailure(error) => {
                    log::error!("output_failure={error}");
                    self.overlay_error = Some(error.to_string());
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
            return;
        }
        let result = match encoded {
            Ok(png) => {
                log::info!(
                    "output_publish target={:?} bytes={}",
                    job.target(),
                    png.len()
                );
                arcshot_app::publish_output(
                    job.clone(),
                    &png,
                    &mut self.pending_save,
                    |png| match native::write_png_to_clipboard(png) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            log::error!("clipboard_failure={error}");
                            Err(OutputFailure::Clipboard)
                        }
                    },
                    |path, png| match native::write_png_to_file(path, png) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            log::error!("save_file_failure={error}");
                            Err(OutputFailure::SaveFile)
                        }
                    },
                )
            }
            Err(error) => {
                self.pending_save.clear();
                Err(error)
            }
        };
        self.action(CaptureAction::OutputCompleted { job, result }, cx);
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
        let busy = self.session.output_busy();
        let error = self.overlay_error.take();
        let updated = handle.update(cx, |view, _, cx| {
            view.selection = selection;
            view.tool = tool;
            view.selected = selected;
            view.annotations = annotations;
            view.rectangle_style = rectangle_style;
            view.line_style = line_style;
            view.arrow_style = arrow_style;
            view.blur_style = blur_style;
            view.busy = busy;
            view.error = error;
            cx.notify();
        });
        if updated.is_err() {
            self.overlay = None;
        }
    }

    fn submit(&mut self, ticket: CaptureTicket, cx: &mut Context<Self>) {
        // All own visible windows are fixed before submission. No other UI is created
        // until the result is resolved, and the status item cannot reopen a native menu.
        cx.global_mut::<MenuBarGlobal>().0.hide_for_capture();
        let prepared = native::builtin_display()
            .and_then(|display| native::owned_visible_window_ids().map(|ids| (display, ids)));
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
            let result = native::capture_builtin(display, ids).map_err(|error| {
                log::error!("capture_failure={error}");
                classify_native_failure(&error)
            })?;
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
        let Some(frame) = self.session.frame().cloned() else {
            return;
        };
        let screen = frame.geometry();
        let preview = cx.background_executor().spawn(async move {
            let preview = arcshot_image::capture_preview(&frame).map_err(|e| e.to_string())?;
            let buffer = image::RgbaImage::from_raw(preview.width, preview.height, preview.bgra)
                .ok_or_else(|| "invalid preview layout".to_owned())?;
            Ok::<_, String>(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
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
                        match state.open_frozen(ticket, screen, image, cx) {
                            Ok(()) => {
                                log::info!(
                                    "capture_wake_window_submit_us={} pixels={:?}",
                                    state
                                        .started
                                        .map(|s| s.elapsed().as_micros())
                                        .unwrap_or_default(),
                                    screen.pixel_size()
                                );
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

    fn open_preparing(
        &mut self,
        ticket: CaptureTicket,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.close_overlay(cx);
        let queue = Rc::clone(&self.queue);
        let bounds = WindowBounds::centered(size(px(420.0), px(110.0)), cx);
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
                    preview: None,
                    hovered_tool: None,
                    focus: focus.clone(),
                    queue,
                });
                window.focus(&focus, cx);
                entity
            },
        )?);
        cx.activate(true);
        Ok(())
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
                    preview: None,
                    hovered_tool: None,
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
                let window_id = window.window_handle().window_id();
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
        CaptureAction::DeleteSelected { .. } => "DeleteSelected",
        CaptureAction::DeselectSelected { .. } => "DeselectSelected",
        CaptureAction::Undo { .. } => "Undo",
        CaptureAction::Redo { .. } => "Redo",
        CaptureAction::Finish { .. } => "Finish",
        CaptureAction::FinishAt { .. } => "FinishAt",
        CaptureAction::OutputCompleted { .. } => "OutputCompleted",
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
