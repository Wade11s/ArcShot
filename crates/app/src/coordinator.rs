//! Connects the authoritative session to safe native operations and GPUI windows.
use crate::{
    MenuBarGlobal,
    ui::{CaptureView, NoticeView, UiCommand, UiQueue},
};
use arcshot_app::{
    CaptureSubmitError, CaptureTasks, GuideHistory, PermissionAction, PermissionEffect,
    PermissionFlow,
};
use arcshot_core::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
    CapturedFrame, FrameError, ScreenGeometry,
};
use arcshot_macos::{self as native, DisplaySnapshot, MenuCommand};
use gpui::{
    App, AppContext, Bounds, Context, Point, RenderImage, Task, Timer, WindowBounds, WindowHandle,
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

struct Coordinator {
    session: CaptureSession,
    tasks: CaptureTasks,
    permissions: PermissionFlow,
    queue: UiQueue,
    overlay: Option<WindowHandle<CaptureView>>,
    notice: Option<WindowHandle<NoticeView>>,
    pending: Option<(CaptureTicket, Instant)>,
    preview_task: Option<Task<()>>,
    started: Option<Instant>,
    settings_departed: bool,
    visiting_settings: bool,
}
pub(crate) fn launch(cx: &mut App) {
    let coordinator = cx.new(|_| Coordinator {
        session: CaptureSession::default(),
        tasks: CaptureTasks::default(),
        permissions: PermissionFlow::new(if native::permission_guide_seen() {
            GuideHistory::Handled
        } else {
            GuideHistory::Unseen
        }),
        queue: Rc::new(RefCell::new(Vec::new())),
        overlay: None,
        notice: None,
        pending: None,
        preview_task: None,
        started: None,
        settings_departed: false,
        visiting_settings: false,
    });
    coordinator.update(cx, |state, cx| {
        state.permission(PermissionAction::Launch, cx)
    });
    // The foreground task owns the coordinator; workers never own GPUI or AppKit state.
    cx.spawn(async move |cx| {
        loop {
            Timer::after(POLL_INTERVAL).await;
            if coordinator.update(cx, |state, cx| state.tick(cx)).is_err() {
                break;
            }
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
                MenuCommand::StartCapture => {
                    self.permission(PermissionAction::Cancel, cx);
                    self.started = Some(Instant::now());
                    self.action(CaptureAction::Start, cx);
                }
                MenuCommand::ScreenRecordingPermissions if !self.busy() => {
                    self.action(CaptureAction::Cancel, cx);
                    self.permission(PermissionAction::Check, cx);
                }
                _ => {}
            }
        }
        let queued = std::mem::take(&mut *self.queue.borrow_mut());
        for command in queued {
            match command {
                UiCommand::CancelCapture(ticket) => {
                    self.action(CaptureAction::CancelAttempt(ticket), cx)
                }
                UiCommand::Permission(window_id, action)
                    if !self.busy()
                        && self
                            .notice
                            .is_some_and(|notice| notice.window_id() == window_id) =>
                {
                    self.permission(action, cx)
                }
                _ => {}
            }
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

    fn action(&mut self, action: CaptureAction, cx: &mut Context<Self>) {
        let mut effects: VecDeque<_> = self.session.dispatch(action).into();
        while let Some(effect) = effects.pop_front() {
            match effect {
                CaptureEffect::CheckPermission(ticket) => {
                    effects.extend(self.session.dispatch(CaptureAction::PermissionChecked {
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
                        effects.extend(self.session.dispatch(CaptureAction::Completed {
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
            }
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
        if let Some(handle) = self.overlay.take() {
            let _ = handle.update(cx, |view, window, cx| {
                if let Some(image) = view.image.take() {
                    cx.drop_image(image, Some(window));
                }
                window.remove_window();
            });
        }
    }
    fn close_notice(&mut self, cx: &mut Context<Self>) {
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
                let entity = cx.new(|cx| CaptureView {
                    ticket,
                    image: None,
                    focus: cx.focus_handle(),
                    queue,
                });
                entity.update(cx, |view, _| window.focus(&view.focus));
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
        let [_, _, w, h] = screen.source_rect();
        let display_id = cx
            .displays()
            .into_iter()
            .find(|display| u32::from(display.id()) == screen.display_id())
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
                let entity = cx.new(|cx| CaptureView {
                    ticket,
                    image: Some(image),
                    focus: cx.focus_handle(),
                    queue,
                });
                entity.update(cx, |view, _| window.focus(&view.focus));
                entity
            },
        )?);
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
            self.open_notice(None, cx);
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
        self.open_notice(Some(message), cx);
    }
    fn open_notice(&mut self, error: Option<String>, cx: &mut Context<Self>) {
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
                let entity = cx.new(|cx| NoticeView {
                    notice,
                    error,
                    focus: cx.focus_handle(),
                    queue,
                });
                entity.update(cx, |view, _| window.focus(&view.focus));
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

/// Text-only corner/center probe of an admitted frame for the manual color
/// oracle. BGRA storage is reported as RRGGBBAA hex; values only, never pixels.
fn capture_probe(frame: &CapturedFrame) -> String {
    let [w, h] = frame.geometry().pixel_size().map(|v| v as usize);
    let stride = frame.stride();
    let px = |x: usize, y: usize| -> String {
        let i = y * stride + x * 4;
        let p = &frame.bgra()[i..i + 4];
        format!("{:02x}{:02x}{:02x}{:02x}", p[2], p[1], p[0], p[3])
    };
    format!(
        "capture_probe_rgb_hex tl={} tr={} bl={} br={} center={}",
        px(0, 0),
        px(w - 1, 0),
        px(0, h - 1),
        px(w - 1, h - 1),
        px(w / 2, h / 2)
    )
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
    fn probe_reports_rgb_corners_and_ignores_padding() {
        // BGRA storage, row stride 12 with 4 padding bytes per row.
        let bytes = vec![
            0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0xff, 0xEE, 0xEE, 0xEE, 0xEE, //
            0xff, 0x00, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0xEE, 0xEE, 0xEE, 0xEE,
        ];
        let report = capture_probe(&frame(bytes, 12));
        assert_eq!(
            report,
            "capture_probe_rgb_hex tl=ff0000ff tr=000000ff bl=0000ffff br=00ff00ff center=00ff00ff"
        );
    }
}
