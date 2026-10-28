//! THROWAWAY native feasibility probe. No production modules, no screenshot files.
mod capture;
mod fixture;
mod native;
mod stitch;

use anyhow::{Result, ensure};
use gpui::{prelude::*, *};
use native::{Input, Point, Rect, Size as NativeSize, Tap};
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
actions!(scroll_probe, [Quit]);

struct Controller {
    pid: u32,
    rect: Rect,
    shared: capture::State,
    input: Arc<Input>,
    tap: Option<Tap>,
    worker: Option<thread::JoinHandle<()>>,
    image: Option<Arc<RenderImage>>,
    message: String,
    encoding: bool,
    since: Option<Instant>,
}
impl Controller {
    fn new(pid: u32, rect: Rect, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        Self {
            pid, rect, shared: Arc::new(Mutex::new(capture::Shared::default())),
            input: Arc::new(Input::default()), tap: None, worker: None, image: None,
            message: "PREPARED — no capture yet.\n1. Grant permissions in Settings.\n2. Start; pointer over target.\n3. Scroll down/up, then Enter.\nM/B/R/J in target inject failures.".into(),
            encoding: false, since: None,
        }
    }
    fn start(&mut self, window: &mut Window) -> Result<()> {
        ensure!(
            self.worker.is_none() && !self.encoding,
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
        let tap = Tap::new(self.input.clone(), self.rect)?;
        native::activate_target(self.pid)?;
        native::preview_window(window, true)?;
        self.tap = Some(tap);
        let shared = self.shared.clone();
        let input = self.input.clone();
        let rect = self.rect;
        let pid = self.pid;
        self.worker = Some(thread::spawn(move || {
            capture::run(shared, input, rect, pid)
        }));
        self.since = Some(Instant::now());
        self.message = "Starting capture; Esc cancels".into();
        Ok(())
    }
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tap.is_some()
            && self.input.command.load(Ordering::SeqCst) == 0
            && !native::target_is_foreground(self.pid)
        {
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
            && let Err(e) = self.start(window)
        {
            self.message = e.to_string();
        }
        cx.notify();
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
                        if let Err(e) = this.start(window) { this.message = format!("{e:#}"); }
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
    if let Ok(value) = std::env::var("SCREENCAPTUREKIT_COMPLETION_TIMEOUT_SECS") {
        ensure!(
            value.parse::<u64>().is_ok_and(|n| (1..=60).contains(&n)),
            "prototype requires a finite 1–60 second native completion timeout"
        );
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--preflight") {
        println!(
            "permissions={:?} display={:?}",
            native::permissions(),
            native::display()
        );
        return Ok(());
    }
    let is_fixture = args.iter().any(|a| a == "--fixture");
    let smoke = args.iter().any(|a| a == "--smoke");
    let left = args.iter().any(|a| a == "--left");
    let inside = args.iter().any(|a| a == "--inside");
    let fixture_x = if left { 340.0 } else { 60.0 };
    let rect = Rect {
        origin: Point {
            x: fixture_x + 16.0,
            y: 116.0,
        },
        size: NativeSize {
            width: 680.0,
            height: 540.0,
        },
    };
    let mut child = if is_fixture {
        None
    } else {
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command.arg("--fixture");
        if smoke {
            command.arg("--smoke");
        }
        if left {
            command.arg("--left");
        }
        Some(command.spawn()?)
    };
    let pid = child.as_ref().map(|c| c.id()).unwrap_or(0);
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
        if is_fixture {
            if let Err(e) = cx.open_window(options, |window, cx| {
                cx.new(|cx| fixture::Fixture::new(window, cx))
            }) {
                eprintln!("fixture window: {e:#}");
                cx.quit();
            }
        } else if let Err(e) = cx.open_window(options, |window, cx| {
            cx.new(|cx| Controller::new(pid, rect, window, cx))
        }) {
            eprintln!("controller window: {e:#}");
            cx.quit();
        }
        cx.activate(true);
        if smoke {
            println!("prepared_window_created fixture={is_fixture}; no capture started");
        }
    });
    if let Some(child) = &mut child {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}
