//! THROWAWAY native feasibility probe. No production modules, no screenshot files.
mod capture;
mod fixture;
mod geometry;
mod native;
mod native_probe;
mod pixel_diagnostics;
mod pixels;
mod stitch;

use anyhow::{Result, ensure};
use geometry::PreviewPlacement;
use gpui::{prelude::*, *};
use native::{Input, Tap};
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
actions!(scroll_probe, [Quit]);

struct Controller {
    pid: u32,
    target: WindowHandle<fixture::Fixture>,
    preview_id: u32,
    placement: PreviewPlacement,
    shared: capture::State,
    input: Arc<Input>,
    tap: Option<Tap>,
    worker: Option<thread::JoinHandle<()>>,
    image: Option<Arc<RenderImage>>,
    message: String,
    encoding: bool,
    since: Option<Instant>,
    preparing_selection: Option<native::Rect>,
    pending_start: Option<Instant>,
}
impl Controller {
    fn new(
        pid: u32,
        target: WindowHandle<fixture::Fixture>,
        placement: PreviewPlacement,
        auto_cancel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let preview_id = native::window_number(window).unwrap_or(0);
        println!(
            "controller_window={preview_id} bounds={:?}",
            window.bounds()
        );
        let input = Arc::new(Input::default());
        input.preview_id.store(preview_id, Ordering::SeqCst);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(100)).await;
                if this
                    .update_in(cx, |this, window, cx| this.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        if auto_cancel {
            cx.spawn_in(window, async move |this, cx| {
                Timer::after(Duration::from_secs(1)).await;
                let started = this.update_in(cx, |this, window, cx| this.start(window, cx));
                println!("auto_probe_start={started:?}");
                if started.is_err() {
                    let _ = cx.update(|_, cx| cx.quit());
                    return;
                }
                Timer::after(Duration::from_secs(2)).await;
                let _ = this.update(cx, |this, _| this.input.command.store(2, Ordering::SeqCst));
                Timer::after(Duration::from_secs(1)).await;
                let cancelled = this
                    .update(cx, |this, _| this.message.starts_with("Cancelled"))
                    .unwrap_or(false);
                println!("auto_probe_cancelled={cancelled}");
                let _ = cx.update(|_, cx| cx.quit());
            })
            .detach();
        }
        Self {
            pid, target, preview_id, placement, shared: Arc::new(Mutex::new(capture::Shared::default())),
            input, tap: None, worker: None, image: None,
            message: "PREPARED — no capture yet.\n1. Grant permissions in Settings.\n2. Start; pointer over target.\n3. Scroll down/up, then Enter.\nM/B/R/J in target inject failures.".into(),
            encoding: false, since: None, preparing_selection: None, pending_start: None,
        }
    }
    fn start<C: AppContext>(&mut self, window: &mut Window, cx: &mut C) -> Result<()> {
        ensure!(
            self.worker.is_none() && self.pending_start.is_none() && !self.encoding,
            "wait for current cleanup/encoding"
        );
        ensure!(
            !self
                .shared
                .lock()
                .expect("prototype state poisoned")
                .requires_relaunch,
            "native cleanup not confirmed: relaunch required"
        );
        if let Some(image) = self.image.take() {
            let _ = window.drop_image(image);
        }
        self.shared = Arc::new(Mutex::new(capture::Shared::default()));
        self.input = Arc::new(Input::default());
        self.input
            .preview_id
            .store(self.preview_id, Ordering::SeqCst);
        let tap = Tap::new(self.input.clone())?;
        native::activate_target(self.pid)?;
        self.target
            .update(cx, |fixture, window, _| {
                fixture.activate_for_capture(window)
            })
            .map_err(|error| anyhow::anyhow!("fixture window unavailable: {error}"))?;
        native::preview_window(window, true)?;
        self.tap = Some(tap);
        self.pending_start = Some(Instant::now());
        self.since = Some(Instant::now());
        self.message = "Preparing fixture keyboard focus; Esc cancels".into();
        Ok(())
    }
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.preview_placed.load(Ordering::SeqCst) {
            if self.input.command.load(Ordering::SeqCst) == 0
                && let Some(rect) = self.preparing_selection.take()
                && let Some(tap) = &mut self.tap
            {
                tap.prepare_selection(rect);
                println!("input_and_capture_selection={rect:?}");
            }
            self.preparing_selection = None;
        }
        if let Some(waiting) = self.pending_start {
            if self.input.command.load(Ordering::SeqCst) != 0 {
                self.pending_start = None;
                self.since = None;
                let command = self.input.command.load(Ordering::SeqCst);
                let mut state = self.shared.lock().expect("prototype state poisoned");
                state.restart = command == 3;
                state.status = match command {
                    2 | 3 => "Cancelled during focus preparation; no clipboard write.".into(),
                    1 => "Stopped before first verified frame; no output.".into(),
                    5 => "STOPPED: input protection lost during preparation; no output.".into(),
                    _ => "Stopped during focus preparation; no output.".into(),
                };
            } else if self
                .target
                .update(cx, |_, window, _| {
                    native::window_is_key(window).unwrap_or(false)
                })
                .unwrap_or(false)
                && native::target_is_foreground(self.pid)
            {
                println!("fixture_key_window_ready");
                let shared = self.shared.clone();
                let input = self.input.clone();
                let pid = self.pid;
                self.worker = Some(thread::spawn(move || capture::run(shared, input, pid)));
                self.pending_start = None;
                self.message = "Preparing capture selection; Esc cancels".into();
            } else if waiting.elapsed() > Duration::from_secs(3) {
                eprintln!("fixture did not become key window during preparation");
                self.pending_start = None;
                self.since = None;
                self.input.command.store(4, Ordering::SeqCst);
                self.shared.lock().expect("prototype state poisoned").status =
                    "STOPPED: fixture did not become key window during preparation; no verified image.".into();
            }
        }
        if self.tap.is_some()
            && self.pending_start.is_none()
            && self.input.command.load(Ordering::SeqCst) == 0
            && (!native::target_is_foreground(self.pid)
                || !self
                    .target
                    .update(cx, |_, window, _| {
                        native::window_is_key(window).unwrap_or(false)
                    })
                    .unwrap_or(false))
        {
            println!(
                "capture_focus_lost expected={} actual={:?}",
                self.pid,
                native::foreground_pid()
            );
            self.input.command.store(6, Ordering::SeqCst);
        }
        let mut restart = self
            .shared
            .lock()
            .expect("prototype state poisoned")
            .restart;
        if let Some(worker) = &self.worker
            && worker.is_finished()
        {
            let worker = self.worker.take().expect("checked worker exists");
            if worker.join().is_err() {
                self.message = "Worker panicked; quit/relaunch required, no output accepted".into();
                self.input.command.store(4, Ordering::SeqCst);
                let mut state = self.shared.lock().expect("prototype state poisoned");
                state.requires_relaunch = true;
                state.status = self.message.clone();
                state.png = None;
                state.result = None;
                state.preview = None;
            } else {
                let state = self.shared.lock().expect("prototype state poisoned");
                self.message = state.status.clone();
                restart = state.restart;
                // A completed capture or failure must release interception, even if no key
                // command caused it. Normal key/button releases are drained first.
                let _ =
                    self.input
                        .command
                        .compare_exchange(0, 4, Ordering::SeqCst, Ordering::SeqCst);
            }
            self.since = None;
        }
        if self
            .since
            .is_some_and(|t| t.elapsed() > Duration::from_secs(180))
        {
            let _ = self
                .input
                .command
                .compare_exchange(0, 4, Ordering::SeqCst, Ordering::SeqCst); // experiment timeout, NOT V1 policy
        }
        if self.tap.as_mut().is_some_and(Tap::drained) {
            self.tap = None;
            if let Err(e) = native::preview_window(window, false) {
                self.message = e.to_string();
            }
        }
        let selection = self
            .shared
            .lock()
            .expect("prototype state poisoned")
            .selection
            .take();
        if let Some(rect) = selection
            && self.input.command.load(Ordering::SeqCst) == 0
            && self.tap.is_some()
        {
            self.prepare_preview(rect, window);
        }
        let mut state = self.shared.lock().expect("prototype state poisoned");
        if matches!(self.input.command.load(Ordering::SeqCst), 2 | 3) {
            state.preview = None;
            state.png = None;
            state.result = None;
            if let Some(image) = self.image.take() {
                let _ = window.drop_image(image);
            }
        }
        if !state.status.is_empty() {
            self.message = state.status.clone();
        }
        if let Some(frame) = state.preview.take() {
            if let Some(old) = self.image.take() {
                let _ = window.drop_image(old);
            }
            self.image = Some(fixture::render_image(frame));
        }
        if !state.requires_relaunch
            && self.tap.is_none()
            && self.worker.is_none()
            && !self.encoding
            && let Some(png) = state.png.take()
        {
            match native::copy_png(&png) {
                Ok(()) => {
                    state.status =
                        "COPIED PNG. No screenshot file saved.\nStart again, or discard.".into();
                    println!("clipboard_png_write=accepted bytes={}", png.len());
                }
                Err(e) => {
                    state.status = format!("Clipboard failed: {e}. Retry or discard.");
                }
            }
        }
        drop(state);
        if restart
            && self.worker.is_none()
            && self.tap.is_none()
            && let Err(e) = self.start(window, cx)
        {
            self.message = e.to_string();
        }
        cx.notify();
    }
    fn prepare_preview(&mut self, rect: native::Rect, window: &mut Window) {
        if self.input.command.load(Ordering::SeqCst) != 0 {
            return;
        }
        let position = native::display()
            .and_then(|(_, screen)| geometry::preview_frame(rect, screen, self.placement))
            .and_then(|frame| native::schedule_move_preview(window, frame, self.input.clone()));
        match position {
            Ok(()) => self.preparing_selection = Some(rect),
            Err(error) => {
                eprintln!("selection preparation failed: {error:#}");
                let _ =
                    self.input
                        .command
                        .compare_exchange(0, 4, Ordering::SeqCst, Ordering::SeqCst);
            }
        }
    }
    fn encode_partial(&mut self) {
        if self.worker.is_some() || self.tap.is_some() || self.encoding {
            return;
        }
        let shared = self.shared.clone();
        self.encoding = true;
        self.worker = Some(thread::spawn(move || capture::encode(&shared)));
        // The same JoinHandle fence prevents a new session or clipboard output until done.
        self.encoding = false;
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.input.command.store(2, Ordering::SeqCst);
        self.tap = None;
    }
}
impl Render for Controller {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.worker.is_some() || self.tap.is_some();
        let blocked = self
            .shared
            .lock()
            .expect("prototype state poisoned")
            .requires_relaunch;
        let can_copy = self
            .shared
            .lock()
            .expect("prototype state poisoned")
            .result
            .is_some();
        div().id("probe-panel").size_full().overflow_y_scroll().flex().flex_col().p_3().gap_2().bg(rgb(0x162033))
            .text_color(rgb(0xffffff)).text_sm()
            .child("ArcShot — THROWAWAY native probe")
            .child(format!("permissions (screen/listen/AX): {:?}", native::permissions()))
            .child(self.message.clone())
            .when_some(self.image.clone(), |d, image| d.child(img(image).max_w(px(180.)).max_h(px(180.)).object_fit(ObjectFit::Contain)))
            .when(!busy && !blocked, |d| d
                .child(div().id("permissions").p_2().bg(rgb(0x354766)).child("Request screen/input permissions")
                    .on_click(|_, _, _| native::request_permissions()))
                .child(div().id("start").p_2().bg(rgb(0x176e52)).child("Start / new session")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Err(e) = this.start(window,cx) {
                            eprintln!("capture_start_rejected: {e:#}");
                            this.message = format!("{e:#}");
                        }
                        cx.notify();
                    })))
                .when(can_copy, |d| d
                    .child(div().id("copy").p_2().bg(rgb(0x784a10)).child("Copy verified partial / retry PNG")
                        .on_click(cx.listener(|this, _, _, cx| { this.encode_partial(); cx.notify(); })))
                    .child(div().id("discard").p_2().bg(rgb(0x583449)).child("Discard result")
                        .on_click(cx.listener(|this, _, window, cx| {
                            *this.shared.lock().expect("prototype state poisoned") = capture::Shared::default();
                            if let Some(image) = this.image.take() { let _ = window.drop_image(image); }
                            this.message = "Discarded. Original selection retained.".into();
                            cx.notify();
                        })))))
            .child("Cmd-Q: quit; capture limit: 180 s.\nExact integer matching only; rejection is expected on fractional/dynamic content.")
    }
}
fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Warn)
        .init();
    if let Ok(value) = std::env::var("SCREENCAPTUREKIT_COMPLETION_TIMEOUT_SECS") {
        ensure!(
            value.parse::<u64>().is_ok_and(|n| (1..=60).contains(&n)),
            "prototype requires a finite 1–60 second native completion timeout"
        );
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--probe-color") {
        return pixels::probe();
    }
    if args.iter().any(|a| a == "--probe-text") {
        let application = Application::new();
        let text_system = application.text_system();
        let id = text_system.resolve_font(&font("Helvetica"));
        let narrow = text_system.advance(id, px(16.), 'i')?.width;
        let wide = text_system.advance(id, px(16.), 'W')?.width;
        println!("text_backend Helvetica i={narrow:?} W={wide:?}");
        ensure!(
            wide > narrow,
            "inactive/synthetic text backend: enable GPUI font-kit"
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--preflight") {
        println!(
            "permissions={:?} display={:?}",
            native::permissions(),
            native::display()
        );
        return Ok(());
    }
    let probe_pixels = args.iter().any(|a| a == "--probe-pixels");
    let probe_exit = Arc::new(std::sync::atomic::AtomicI32::new(-1));
    let result_exit = probe_exit.clone();
    let probe_inset = args
        .iter()
        .position(|a| a == "--probe-inset")
        .map(|i| -> Result<usize> {
            Ok(args
                .get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("missing probe inset"))?
                .parse()?)
        })
        .transpose()?
        .unwrap_or(40);
    let auto_cancel = args.iter().any(|a| a == "--auto-cancel");
    let is_fixture = probe_pixels || args.iter().any(|a| a == "--fixture");
    let smoke = args.iter().any(|a| a == "--smoke");
    let left = args.iter().any(|a| a == "--left");
    let inside = args.iter().any(|a| a == "--inside");
    let fixture_x = if left { 340.0 } else { 60.0 };
    let placement = if inside {
        PreviewPlacement::Inside
    } else if left {
        PreviewPlacement::Left
    } else {
        PreviewPlacement::Right
    };
    if let Some(index) = args.iter().position(|arg| arg == "--probe-target") {
        let pid: u32 = args
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("missing target pid"))?
            .parse()?;
        return capture::probe_target(pid);
    }
    let pid = std::process::id();
    println!(
        "THROWAWAY prototype pid={} target_pid={pid}; no networking and no image persistence requested by this program",
        std::process::id()
    );
    Application::new().run(move |cx: &mut App| {
        if smoke {
            cx.spawn(async move |cx| {
                Timer::after(Duration::from_secs(3)).await;
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        }
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let (x, y, w, h) = if is_fixture {
            (fixture_x, 100.0, 720.0, 600.0)
        } else if inside {
            (
                fixture_x + 16.0 + 680.0 - 240.0,
                116.0 + 540.0 - 460.0,
                240.0,
                460.0,
            )
        } else if left {
            (60.0, 100.0, 240.0, 560.0)
        } else {
            (800.0, 100.0, 240.0, 560.0)
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(x as f32), px(y as f32)),
                size(px(w as f32), px(h as f32)),
            ))),
            titlebar: None,
            is_resizable: false,
            is_movable: false,
            ..Default::default()
        };
        let mut target_window = None;
        if !is_fixture {
            let fixture_options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(fixture_x as f32), px(100.)),
                    size(px(720.), px(600.)),
                ))),
                titlebar: None,
                is_resizable: false,
                is_movable: false,
                ..Default::default()
            };
            match cx.open_window(fixture_options, |window, cx| {
                cx.new(|cx| fixture::Fixture::new(window, cx))
            }) {
                Ok(handle) => target_window = Some(handle),
                Err(e) => {
                    eprintln!("controlled fixture window: {e:#}");
                    cx.quit();
                }
            }
        }
        if is_fixture {
            if let Err(e) = cx.open_window(options, |window, cx| {
                if probe_pixels {
                    native_probe::schedule(window, cx, probe_exit.clone(), probe_inset);
                }
                cx.new(|cx| {
                    let mut fixture = fixture::Fixture::new(window, cx);
                    if probe_pixels {
                        fixture.freeze_for_probe();
                    }
                    fixture
                })
            }) {
                eprintln!("fixture window: {e:#}");
                cx.quit();
            }
        } else if let Some(target) = target_window
            && let Err(e) = cx.open_window(options, |window, cx| {
                cx.new(|cx| Controller::new(pid, target, placement, auto_cancel, window, cx))
            })
        {
            eprintln!("controller window: {e:#}");
            cx.quit();
        }
        cx.activate(true);
        if smoke {
            println!("prepared_window_created fixture={is_fixture}; no capture started");
        }
    });
    if probe_pixels {
        ensure!(
            result_exit.load(Ordering::SeqCst) == 0,
            "native pixel probe failed (see numeric diagnostics)"
        );
    }
    Ok(())
}
