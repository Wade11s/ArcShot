//! One outstanding SCK screenshot request at a time; no persistent SCStream.
use crate::{
    native::{self, Input, Rect},
    stitch::{Frame, MAX_IMAGE_BYTES, Stitch},
};
use anyhow::{Context, Result, bail, ensure};
use screencapturekit::{
    cg::CGRect,
    prelude::*,
    screenshot_manager::{CGImageExt, SCScreenshotManager},
    stream::configuration::color_space,
};
use std::{
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Shared {
    pub status: String,
    pub preview: Option<Frame>,
    pub result: Option<Frame>,
    pub png: Option<Vec<u8>>,
    pub restart: bool,
    pub requires_relaunch: bool,
}
pub type State = Arc<Mutex<Shared>>;

pub fn run(shared: State, input: Arc<Input>, rect: Rect, target_pid: u32) {
    let started = Instant::now();
    let mut stitched = None;
    let result = collect(&shared, &input, rect, target_pid, &mut stitched);
    let command = input.command.load(Ordering::SeqCst);
    let completed = result.is_ok() && command == 1 && !input.protection_lost.load(Ordering::SeqCst);
    println!("process_peak_rss_bytes={}", native::peak_rss_bytes());
    let mut state = shared.lock().expect("prototype shared state poisoned");
    if state.requires_relaunch {
        state.status = format!(
            "Native request failed/unconfirmed: {result:?}. Quit and relaunch; output/restart disabled."
        );
        state.preview = None;
        state.result = None;
        state.png = None;
    } else if matches!(command, 2 | 3) {
        state.status = "Cancelled; selection retained. No clipboard write.".into();
        state.restart = command == 3;
    } else {
        match result {
            Ok(()) if completed => {
                state.result = stitched.map(|s: Stitch| s.image);
                state.status = if state.result.is_some() {
                    "Capture stopped; encoding explicit Finish...".into()
                } else {
                    "No verified initial frame yet; no output. Start again and wait for the preview.".into()
                };
            }
            Ok(()) => {
                state.status = format!(
                    "Stopped without output; command={command} (5=lost input protection, 6=focus lost)"
                );
                state.result = stitched.map(|s: Stitch| s.image);
            }
            Err(error) => {
                state.status =
                    format!("STOPPED: {error:#}. Use verified partial explicitly, or discard.");
                state.result = stitched.map(|s: Stitch| s.image);
            }
        }
    }
    println!(
        "session_seconds={:.3} command={command} scroll_events={} intercepted_clicks={} {}",
        started.elapsed().as_secs_f64(),
        input.scrolls.load(Ordering::Relaxed),
        input.clicks.load(Ordering::Relaxed),
        state.status
    );
    let auto = completed && !state.requires_relaunch && state.result.is_some();
    drop(state);
    if auto {
        encode(&shared);
    }
}
pub fn encode(shared: &State) {
    let image = shared
        .lock()
        .expect("prototype shared state poisoned")
        .result
        .take();
    if let Some(image) = image {
        let start = Instant::now();
        let encoded = image.png();
        let mut state = shared.lock().expect("prototype shared state poisoned");
        match encoded {
            Ok(png) => {
                println!(
                    "encoded_png_bytes={} encode_ms={} image={}x{} encode_peak_rss_bytes={}",
                    png.len(),
                    start.elapsed().as_millis(),
                    image.width,
                    image.height,
                    native::peak_rss_bytes()
                );
                state.png = Some(png);
                state.status = "PNG ready for clipboard".into();
            }
            Err(e) => state.status = format!("PNG encoding failed: {e}. Retry or discard."),
        }
        state.result = Some(image);
    }
}
fn collect(
    shared: &State,
    input: &Input,
    rect: Rect,
    pid: u32,
    stitched: &mut Option<Stitch>,
) -> Result<()> {
    // Terminal results are fenced outside request/processing scopes. A request racing
    // cancellation cannot publish a late image into a new generation.
    let outcome = collect_inner(shared, input, rect, pid, stitched);
    let _ = input
        .command
        .compare_exchange(0, 4, Ordering::SeqCst, Ordering::SeqCst);
    outcome
}
fn collect_inner(
    shared: &State,
    input: &Input,
    rect: Rect,
    pid: u32,
    stitched: &mut Option<Stitch>,
) -> Result<()> {
    let (display_id, bounds) = native::display()?;
    ensure!(
        bounds.contains(rect.origin)
            && bounds.contains(native::Point {
                x: rect.origin.x + rect.size.width - 0.01,
                y: rect.origin.y + rect.size.height - 0.01,
            }),
        "selection outside built-in display"
    );
    let (content, target) = wait_for_target(shared, input, pid, rect)?;
    let displays = content.displays();
    let display = displays
        .iter()
        .find(|d| d.display_id() == display_id)
        .context("built-in display absent")?;
    let target_id = target.window_id();
    let target_frame = target.frame();
    let apps = content.applications();
    let this_app = apps
        .iter()
        .find(|a| a.process_id() == std::process::id() as i32)
        .context("prototype process absent from SCK; cannot safely exclude preview")?;
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_applications(&[this_app], &[])
        .build()?;
    let scale = f64::from(filter.point_pixel_scale());
    ensure!(
        scale.is_finite() && (1.0..=4.0).contains(&scale) && scale.fract() == 0.0,
        "fixture requires integer capture scale"
    );
    let width = (rect.size.width * scale).round() as usize;
    let height = (rect.size.height * scale).round() as usize;
    ensure!(
        width * height * 4 < MAX_IMAGE_BYTES / 4,
        "viewport exceeds experimental working budget"
    );
    let config = SCStreamConfiguration::new()
        .with_source_rect(CGRect::new(
            rect.origin.x - bounds.origin.x,
            rect.origin.y - bounds.origin.y,
            rect.size.width,
            rect.size.height,
        ))
        .with_width(width as u32)
        .with_height(height as u32)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(false)
        .with_color_space_name(color_space::SRGB)?;
    println!(
        "candidate=SCScreenshotManager.capture_image rect={rect:?} pixels={width}x{height} scale={scale} display={display_id} target={target_id}"
    );
    let mut last_source_check = Instant::now();
    let mut last_preview = Instant::now() - Duration::from_secs(1);
    let mut sequence = 0u64;
    loop {
        match input.command.load(Ordering::SeqCst) {
            1..=3 => return Ok(()),
            4 => bail!("processing stopped or experimental timeout"),
            5 => bail!("input protection lost: tap disabled"),
            6 => bail!("target lost foreground focus"),
            _ => {}
        }
        if last_source_check.elapsed() >= Duration::from_millis(600) {
            let topology = native::display();
            if !topology
                .as_ref()
                .is_ok_and(|(id, b)| *id == display_id && b.size == bounds.size)
            {
                input.command.store(2, Ordering::SeqCst);
                bail!("display topology/geometry changed; entire session discarded");
            }
            ensure!(
                native::permissions() == (true, true, true),
                "permission revoked"
            );
            let current = SCShareableContent::get()?;
            let window = current
                .windows()
                .into_iter()
                .find(|w| w.window_id() == target_id && w.is_on_screen())
                .context("target closed/minimized/unavailable")?;
            ensure!(window.frame() == target_frame, "target moved/resized");
            last_source_check = Instant::now();
        }
        if input.command.load(Ordering::SeqCst) != 0 {
            continue;
        }
        let request_start = Instant::now();
        // The public one-shot API returns an owned CGImage via its completion.
        // No stream delegate, capturing flag, stop callback or persistent stream is used.
        let image = match SCScreenshotManager::capture_image(&filter, &config) {
            Ok(image) => image,
            Err(error) => {
                // Treat every request error as potentially ambiguous; don't retry or output.
                shared
                    .lock()
                    .expect("prototype state poisoned")
                    .requires_relaunch = true;
                return Err(error.into());
            }
        };
        sequence += 1;
        println!(
            "snapshot_sequence={sequence} request_ms={}",
            request_start.elapsed().as_millis()
        );
        // An in-flight request may complete after Finish/cancel. Drop it, retain only
        // previously admitted frames, and wait for this worker to exit before restart.
        if input.command.load(Ordering::SeqCst) != 0 {
            continue;
        }
        ensure!(
            image.width() == width && image.height() == height,
            "snapshot geometry changed"
        );
        let pixels = image.bgra_data()?;
        ensure!(
            pixels.len() == width * height * 4,
            "unexpected snapshot byte size"
        );
        let frame = Frame {
            width,
            height,
            pixels,
        };
        let compute = Instant::now();
        // Fixture-only source validation, not a candidate-offset generator.
        ensure!(
            crate::fixture::oracle(&frame, scale as usize),
            "fixture ground truth mismatch: source, crop, color, exclusion or mutation; validation failed"
        );
        if let Some(stitch) = stitched {
            let delta = stitch.add(frame)?;
            if delta != 0 {
                println!(
                    "fixture_frame_ground_truth_exact=true verified_delta_px={delta} result_height_px={} stitch_ms={}",
                    stitch.image.height,
                    compute.elapsed().as_millis()
                );
            }
        } else {
            println!("fixture_initial_ground_truth_exact=true");
            *stitched = Some(Stitch::new(frame)?);
        }
        if last_preview.elapsed() >= Duration::from_millis(200) {
            if let Some(stitch) = stitched {
                let thumbnail = stitch.image.thumbnail();
                let mut state = shared.lock().expect("prototype state poisoned");
                state.preview = Some(thumbnail);
                state.status = format!(
                    "CAPTURING {} x {} px; samples={sequence}\nEnter / double-click: Finish\nEsc: Cancel | Cmd-Shift-A: restart",
                    stitch.image.width, stitch.image.height
                );
            }
            last_preview = Instant::now();
        }
        // Experimental pacing, not a guaranteed sampling rate. A large scroll gap fails
        // overlap verification rather than being inferred from event deltas.
        thread::sleep(Duration::from_millis(50));
    }
}
pub fn probe_target(pid: u32, rect: Rect) -> Result<()> {
    let content = SCShareableContent::get()?;
    let target = find_target(&content, pid, rect)?;
    println!(
        "target_check=pass window={} rect={:?}",
        target.window_id(),
        target.frame()
    );
    Ok(())
}
fn wait_for_target(
    shared: &State,
    input: &Input,
    pid: u32,
    rect: Rect,
) -> Result<(SCShareableContent, SCWindow)> {
    let started = Instant::now();
    let mut previous = None;
    let mut pinned_id = None;
    shared.lock().expect("prototype state poisoned").status =
        "PREPARING — waiting for target window geometry; Esc cancels".into();
    loop {
        ensure!(
            input.command.load(Ordering::SeqCst) == 0,
            "preparation interrupted"
        );
        let content = SCShareableContent::get()?;
        ensure!(
            input.command.load(Ordering::SeqCst) == 0,
            "preparation interrupted"
        );
        ensure!(
            started.elapsed() < Duration::from_secs(3),
            "target did not become ready within preparation budget; selection unchanged"
        );
        match find_target(&content, pid, rect) {
            Ok(target) => {
                let current = (target.window_id(), target.frame());
                if let Some(id) = pinned_id {
                    ensure!(
                        current.0 == id,
                        "target window identity changed during preparation"
                    );
                } else {
                    pinned_id = Some(current.0);
                }
                if previous == Some(current) {
                    println!(
                        "target_ready window={} frame={:?} preparation_ms={}",
                        current.0,
                        current.1,
                        started.elapsed().as_millis()
                    );
                    return Ok((content, target));
                }
                previous = Some(current);
            }
            Err(_) => previous = None,
        }
        thread::sleep(Duration::from_millis(80));
    }
}
fn find_target(content: &SCShareableContent, pid: u32, rect: Rect) -> Result<SCWindow> {
    let candidates: Vec<_> = content
        .windows()
        .into_iter()
        .filter(|w| {
            w.owning_application()
                .is_some_and(|a| a.process_id() == pid as i32)
        })
        .collect();
    println!(
        "target_probe pid={pid} selection={rect:?} candidates={}",
        candidates.len()
    );
    for w in &candidates {
        println!(
            "target_probe window={} on_screen={} frame={:?}",
            w.window_id(),
            w.is_on_screen(),
            w.frame()
        );
    }
    candidates
        .into_iter()
        .find(|w| w.window_layer() == 0 && w.is_on_screen() && contains(w.frame(), rect))
        .context("test target not visible / selection not inside target window")
}
fn contains(frame: CGRect, rect: Rect) -> bool {
    rect.origin.x >= frame.origin.x
        && rect.origin.y >= frame.origin.y
        && rect.origin.x + rect.size.width <= frame.origin.x + frame.size.width
        && rect.origin.y + rect.size.height <= frame.origin.y + frame.size.height
}
