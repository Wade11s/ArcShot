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

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    App, AppContext, Bounds, Context, FocusHandle, Point, RenderImage, Window, WindowBounds,
    WindowOptions, div, img, px, size,
};

/// Logical window origin on the single built-in display; the E2E drags its
/// selection well inside this rect.
const ORIGIN: (f32, f32) = (600.0, 120.0);
/// Logical window size.
const SIZE: (f32, f32) = (760.0, 720.0);
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
    image: Option<Arc<RenderImage>>,
    focus: FocusHandle,
}

fn build_image(offset: i32, window: &mut Window) -> Arc<RenderImage> {
    let scale = window.scale_factor().round().max(1.0) as usize;
    let width = SIZE.0 as usize * scale;
    let height = SIZE.1 as usize * scale;
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
        self.image = Some(build_image(self.offset, window));
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
                    fixture.offset = (fixture.offset - delta).clamp(0, MAX_OFFSET);
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
                move |event, _, _cx| {
                    eprintln!("fixture_key_received key={:?}", event.keystroke.key);
                    let _ = &entity;
                }
            })
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(px(ORIGIN.0), px(ORIGIN.1)),
                size: size(px(SIZE.0), px(SIZE.1)),
            })),
            // Borderless so the drawn image exactly fills the window frame:
            // the pixel oracle maps captured frame pixels to content one-to-one.
            titlebar: None,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        };
        let result = cx.open_window(options, |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            eprintln!(
                "fixture_window origin=({},{}) size=({},{}) initial_offset_points={}",
                ORIGIN.0, ORIGIN.1, SIZE.0, SIZE.1, INITIAL_OFFSET
            );
            cx.new(|_| Fixture {
                offset: INITIAL_OFFSET,
                image: Some(build_image(INITIAL_OFFSET, window)),
                focus,
            })
        });
        if result.is_ok() {
            cx.activate(true);
        }
    });
}
