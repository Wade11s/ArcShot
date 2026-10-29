//! Agent-runnable pixel probe: only this process's generated fixture; no input or image files.
use crate::{fixture, geometry, native, pixel_diagnostics, pixels, stitch::Frame};
use anyhow::{Context, Result, ensure};
use gpui::{App, Timer, Window};
use screencapturekit::{
    prelude::*,
    screenshot_manager::{CGImageExt, SCScreenshotManager},
    stream::configuration::color_space,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
    time::Duration,
};

pub fn schedule(window: &Window, cx: &mut App, exit: Arc<AtomicI32>, inset: usize) {
    let id = native::window_number(window);
    let executor = cx.background_executor().clone();
    cx.spawn(async move |_cx| {
        Timer::after(Duration::from_secs(1)).await;
        let result = executor.spawn(async move { run(id?, inset) }).await;
        println!("native_pixel_probe_result={result:?}");
        exit.store(if result.is_ok() { 0 } else { 1 }, Ordering::SeqCst);
        // This diagnostic mode has no child/input tap or ongoing capture. All image
        // objects were released when run returned; preserve a reliable CLI exit code.
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    })
    .detach();
}
fn check(label: &str, frame: &Frame, scale: usize, inset: usize) -> bool {
    let valid = fixture::oracle(frame, scale);
    println!(
        "native_pixel_probe {label} dimensions={}x{} exact_oracle={valid}",
        frame.width, frame.height
    );
    if !valid {
        let base = (2000 + inset) * scale;
        let mut bad_rows = Vec::new();
        for y in 0..frame.height {
            let matches = (0..frame.width).all(|x| {
                let at = (y * frame.width + x) * 4;
                frame.pixels[at..at + 4] == fixture::pixel(16 * scale + x, base + y, scale)
            });
            if !matches {
                bad_rows.push(y);
            }
        }
        println!(
            "native_pixel_probe {label} initial_reference_bad_rows={} first={:?} last={:?}",
            bad_rows.len(),
            bad_rows.first(),
            bad_rows.last()
        );
        for y in [0, 16, 24, 32, 64, frame.height - 1] {
            if y < frame.height {
                let at = y * frame.width * 4;
                println!(
                    "native_pixel_probe {label} lane_row={y} actual={:?} expected={:?}",
                    &frame.pixels[at..at + 4],
                    fixture::pixel(16 * scale, base + y, scale)
                );
            }
        }
        println!(
            "native_pixel_probe {label} {:?}",
            pixel_diagnostics::report(frame, scale)
        );
    }
    valid
}
fn crop(bytes: Vec<u8>, width: usize, height: usize, scale: usize, inset: usize) -> Result<Frame> {
    let (x, y, w, h) = (16 * scale, inset * scale, 680 * scale, 540 * scale);
    ensure!(
        width >= x + w && height >= y + h && bytes.len() == width * height * 4,
        "invalid window-probe crop"
    );
    let mut pixels = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        pixels.extend_from_slice(&bytes[(row * width + x) * 4..(row * width + x + w) * 4]);
    }
    Ok(Frame {
        width: w,
        height: h,
        pixels,
    })
}
fn config(width: usize, height: usize) -> Result<SCStreamConfiguration> {
    Ok(SCStreamConfiguration::new()
        .with_width(width as u32)
        .with_height(height as u32)
        .with_shows_cursor(false)
        .with_pixel_format(PixelFormat::BGRA)
        .with_color_space_name(color_space::SRGB)?)
}
fn run(id: u32, inset: usize) -> Result<()> {
    ensure!(inset <= 60, "probe inset exceeds fixture bounds");
    ensure!(
        native::permissions().0,
        "screen recording not authorized; probe will not request it"
    );
    let content = SCShareableContent::get()?;
    let window = content
        .windows()
        .into_iter()
        .find(|w| {
            w.window_id() == id
                && w.owning_application()
                    .is_some_and(|a| a.process_id() == std::process::id() as i32)
        })
        .context("probe's own fixture window absent")?;
    let frame = window.frame();
    let _ = geometry::fixture_selection(native::Rect {
        origin: native::Point {
            x: frame.origin.x,
            y: frame.origin.y,
        },
        size: native::Size {
            width: frame.size.width,
            height: frame.size.height,
        },
    })?;
    let filter = SCContentFilter::create().with_window(&window).build()?;
    let scale = f64::from(filter.point_pixel_scale());
    ensure!(
        (1.0..=4.0).contains(&scale) && scale.fract() == 0.,
        "non-integer probe scale"
    );
    let scale = scale as usize;
    let (width, height) = (720 * scale, 600 * scale);
    let cfg = config(width, height)?.with_ignores_shadows_single_window(true)?;
    let image = SCScreenshotManager::capture_image(&filter, &cfg)?;
    ensure!(
        image.width() == width && image.height() == height,
        "probe image size mismatch"
    );
    println!(
        "native_pixel_probe own_pid={} own_window={id} frame={frame:?} scale={scale}",
        std::process::id()
    );
    let raw = crop(image.bgra_data()?, width, height, scale, inset)?;
    let window_ok = check("window_DeviceRGB", &raw, scale, inset);
    drop(raw);
    let normalized = crop(pixels::srgb_bgra(&image)?, width, height, scale, inset)?;
    check("window_named_sRGB", &normalized, scale, inset);
    drop(normalized);
    drop(image);
    ensure!(
        window_ok,
        "own-window pixel probe failed exact source validation"
    );
    Ok(())
}
