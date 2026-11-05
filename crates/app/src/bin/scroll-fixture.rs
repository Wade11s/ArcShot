//! ArcShot's own GPUI exact-pixel Scroll test fixture (I-13 target class 1).
//!
//! This is product-test infrastructure, not product surface: the E2E harness
//! launches it as its own process so ArcShot can capture and protect a real
//! third-party-style window whose every pixel is a deterministic function of
//! its offset. The product itself never spawns it and never depends on it; it
//! has no network, no disk writes and no capture code. It renders a
//! deterministic image, moves it with real scroll-wheel events, and prints
//! text-only receipts for the scroll, click and key events it actually
//! receives — the acceptance evidence for click blocking and scroll
//! pass-through at the native seam.
//!
//! The window geometry and scroll range are configurable (`--origin-x`,
//! `--origin-y`, `--width`, `--height` in logical points; `--max-offset`
//! and `--initial-offset` in logical rows) so the 24M document build
//! path (#59) can use a wider window and a scroll range that lands exactly
//! on the 24,000,000-pixel cap. With the default 760-point window a
//! 750-point selection leaves room only for the inner preview placement,
//! which sits inside the captured frame and whose top-edge redraw transient
//! makes strict per-row verification refuse. A wider window lets the same
//! selection start below that transient and use the left preview placement,
//! so the captured frame contains only target content. The defaults keep
//! every existing E2E case working unchanged.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    App, AppContext, Bounds, Context, FocusHandle, Point, RenderImage, Window, WindowBounds,
    WindowOptions, div, img, px, size,
};

/// Default logical window origin on the single built-in display; the E2E
/// drags its selection well inside this rect.
const DEFAULT_ORIGIN: (f32, f32) = (600.0, 120.0);
/// Default logical window size.
const DEFAULT_SIZE: (f32, f32) = (760.0, 720.0);
/// Initial content row offset, in logical rows; deterministic per launch.
const INITIAL_OFFSET: i32 = 2000;
/// Total logical content rows; scrolling clamps to this range.
const MAX_OFFSET: i32 = 7400;

/// The deterministic fixture pixel function.
///
/// Returns the exact bytes the composited window shows for content column `x`
/// and content row `y` at `scale` physical pixels per point. Storage order
/// matches GPUI's macOS BGRA frames, so an admitted ArcShot capture of this
/// window must equal this function's output byte for byte.
#[must_use]
pub fn fixture_pixel(x: usize, y: usize, scale: usize) -> [u8; 4] {
    if x < 64 * scale {
        return [(y & 255) as u8, ((y >> 8) & 255) as u8, 173, 255];
    }
    let block = x / (8 * scale);
    let mut hash = (y as u32).wrapping_mul(0x9e37_79b9)
        ^ (u32::try_from(block).unwrap_or(0)).wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 16;
    let bytes = hash.to_le_bytes();
    [bytes[0], bytes[1], bytes[2], 255]
}

struct Fixture {
    offset: i32,
    geometry: Geometry,
    image: Option<Arc<RenderImage>>,
    focus: FocusHandle,
}

/// Logical window geometry in points, from defaults or `--` options.
#[derive(Clone, Copy, Debug)]
struct Geometry {
    origin: (f32, f32),
    size: (f32, f32),
    /// Maximum content offset in logical rows; the 24M build path (#59)
    /// needs a taller scroll range than the default fixture provides.
    max_offset: i32,
    /// Content offset at startup, in logical rows.
    initial_offset: i32,
}

/// Parses the optional `--origin-x`, `--origin-y`, `--width`, `--height`,
/// `--max-offset` and `--initial-offset` options; everything else keeps its
/// default. Unknown arguments, non-positive sizes or an initial offset
/// outside the scroll range are a usage error, never a guessed geometry.
fn parse_geometry() -> Result<Geometry, String> {
    let mut geometry = Geometry {
        origin: DEFAULT_ORIGIN,
        size: DEFAULT_SIZE,
        max_offset: MAX_OFFSET,
        initial_offset: INITIAL_OFFSET,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            return Err(format!("{flag} needs a value"));
        };
        match flag.as_str() {
            "--origin-x" | "--origin-y" | "--width" | "--height" => {
                let parsed: f32 = value
                    .parse()
                    .map_err(|_| format!("{flag} is not a number: {value}"))?;
                if !parsed.is_finite() {
                    return Err(format!("{flag} is not finite: {value}"));
                }
                match flag.as_str() {
                    "--origin-x" => geometry.origin.0 = parsed,
                    "--origin-y" => geometry.origin.1 = parsed,
                    "--width" => geometry.size.0 = parsed,
                    _ => geometry.size.1 = parsed,
                }
            }
            "--max-offset" | "--initial-offset" => {
                let parsed: i32 = value
                    .parse()
                    .map_err(|_| format!("{flag} is not an integer: {value}"))?;
                if parsed < 0 {
                    return Err(format!("{flag} must not be negative: {value}"));
                }
                if flag == "--max-offset" {
                    geometry.max_offset = parsed;
                } else {
                    geometry.initial_offset = parsed;
                }
            }
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    if geometry.size.0 <= 0.0 || geometry.size.1 <= 0.0 {
        return Err("window width and height must be positive".to_owned());
    }
    if !(0..=geometry.max_offset).contains(&geometry.initial_offset) {
        return Err("initial offset must lie within [0, max offset]".to_owned());
    }
    Ok(geometry)
}

fn build_image(offset: i32, geometry: Geometry, window: &mut Window) -> Arc<RenderImage> {
    let scale = window.scale_factor().round().max(1.0) as usize;
    let width = geometry.size.0 as usize * scale;
    let height = geometry.size.1 as usize * scale;
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = offset as usize * scale + y;
        for x in 0..width {
            let pixel = fixture_pixel(x, row, scale);
            pixels.extend_from_slice(&pixel);
        }
    }
    let buffer = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
        .expect("fixture image is sized from its own loop bounds");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

impl Fixture {
    fn rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(previous) = self.image.take() {
            let _ = window.drop_image(previous);
        }
        self.image = Some(build_image(self.offset, self.geometry, window));
        cx.notify();
    }
}

impl gpui::Render for Fixture {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image = self
            .image
            .clone()
            .expect("the fixture image is rebuilt on every change");
        let entity = cx.entity();
        div()
            .id("arcshot-scroll-fixture")
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .child(img(image).size_full())
            .on_scroll(move |event, window, cx| {
                let delta = f32::from(event.delta.pixel_delta(px(16.)).y).round() as i32;
                entity.update(cx, |fixture, cx| {
                    fixture.offset = (fixture.offset - delta).clamp(0, fixture.geometry.max_offset);
                    eprintln!(
                        "fixture_scroll delta_points={delta} offset_points={}",
                        fixture.offset
                    );
                    fixture.rebuild(window, cx);
                });
            })
            .on_mouse_down(gpui::MouseButton::Left, {
                let entity = cx.entity();
                move |event, _, _cx| {
                    eprintln!(
                        "fixture_click_received button=left at=({:.0},{:.0})",
                        event.position.x, event.position.y
                    );
                    let _ = &entity;
                }
            })
            .on_mouse_down(gpui::MouseButton::Right, {
                let entity = cx.entity();
                move |event, _, _cx| {
                    eprintln!(
                        "fixture_click_received button=right at=({:.0},{:.0})",
                        event.position.x, event.position.y
                    );
                    let _ = &entity;
                }
            })
            .on_key_down({
                let entity = cx.entity();
                move |event, window, cx| {
                    eprintln!("fixture_key_received key={:?}", event.keystroke.key);
                    // #34 controlled negative case: an atomic jump past every
                    // verified row, so the next captured frame has no overlap
                    // with verified history (a Gap stop). Non-owned keys pass
                    // the input protection to the target, like typing.
                    if event.keystroke.key == "j" {
                        entity.update(cx, |fixture, cx| {
                            fixture.offset =
                                (fixture.offset + 1000).min(fixture.geometry.max_offset);
                            eprintln!("fixture_jump offset_points={}", fixture.offset);
                            fixture.rebuild(window, cx);
                        });
                    }
                }
            })
    }
}

fn main() {
    let geometry = match parse_geometry() {
        Ok(geometry) => geometry,
        Err(message) => {
            eprintln!(
                "usage: arcshot-scroll-fixture [--origin-x N] [--origin-y N] [--width N] [--height N] [--max-offset N] [--initial-offset N]"
            );
            eprintln!("error: {message}");
            std::process::exit(2);
        }
    };
    gpui_platform::application().run(move |cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(px(geometry.origin.0), px(geometry.origin.1)),
                size: size(px(geometry.size.0), px(geometry.size.1)),
            })),
            // Borderless so the drawn image exactly fills the window frame:
            // the pixel oracle maps captured frame pixels to content one-to-one.
            titlebar: None,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        };
        let result = cx.open_window(options, move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            eprintln!(
                "fixture_window origin=({},{}) size=({},{}) initial_offset_points={}",
                geometry.origin.0,
                geometry.origin.1,
                geometry.size.0,
                geometry.size.1,
                geometry.initial_offset
            );
            cx.new(|_| Fixture {
                offset: geometry.initial_offset,
                geometry,
                image: Some(build_image(geometry.initial_offset, geometry, window)),
                focus,
            })
        });
        if result.is_ok() {
            cx.activate(true);
        }
    });
}
