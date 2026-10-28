//! Test target ONLY; separate process so SCK can exclude the capture UI, not its input.
use crate::stitch::Frame;
use gpui::{prelude::*, *};
use std::sync::Arc;

pub fn render_image(frame: Frame) -> Arc<RenderImage> {
    let buffer = image::RgbaImage::from_raw(frame.width as u32, frame.height as u32, frame.pixels)
        .expect("internally sized BGRA image");
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}
pub(crate) fn pixel(x: usize, y: usize, scale: usize) -> [u8; 4] {
    if x < 64 * scale {
        return [(y & 255) as u8, ((y >> 8) & 255) as u8, 173, 255];
    }
    let block = x / (8 * scale);
    let mut h = (y as u32).wrapping_mul(0x9e3779b9) ^ (block as u32).wrapping_mul(0x85ebca6b);
    h ^= h >> 16;
    let bytes = h.to_le_bytes();
    [bytes[0], bytes[1], bytes[2], 255]
}
pub fn oracle(frame: &Frame, scale: usize) -> bool {
    if frame.pixels.len() < 4 || scale == 0 {
        return false;
    }
    let y0 = usize::from(frame.pixels[0]) + (usize::from(frame.pixels[1]) << 8);
    frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .all(|(n, p)| *p == pixel(16 * scale + n % frame.width, y0 + n / frame.width, scale))
}

pub struct Fixture {
    offset: i32,
    mode: u8,
    image: Option<Arc<RenderImage>>,
    focus: FocusHandle,
}
impl Fixture {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        println!(
            "fixture_window={:?} bounds={:?}",
            crate::native::window_number(window),
            window.bounds()
        );
        let focus = cx.focus_handle();
        window.focus(&focus);
        println!(
            "fixture initial_offset_points=2000; M=mutate B=blank R=repeat U=unique J=jump gap Q=quit; click/key receipts printed here"
        );
        Self {
            offset: 2000,
            mode: 0,
            image: None,
            focus,
        }
    }
    fn invalidate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(image) = self.image.take() {
            let _ = window.drop_image(image);
        }
        cx.notify();
    }
}
impl Render for Fixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scale = window.scale_factor().round().max(1.0) as usize;
        if self.image.is_none() {
            let width = 720 * scale;
            let height = 600 * scale;
            let mut pixels = Vec::with_capacity(width * height * 4);
            for y in 0..height {
                for x in 0..width {
                    let row = self.offset as usize * scale + y;
                    let p = match self.mode {
                        1 => {
                            let mut p = pixel(x, row, scale);
                            p[2] ^= 127;
                            p
                        }
                        2 => [180, 180, 180, 255],
                        3 => pixel(x, row % (40 * scale), scale),
                        _ => pixel(x, row, scale),
                    };
                    pixels.extend_from_slice(&p);
                }
            }
            self.image = Some(render_image(Frame {
                width,
                height,
                pixels,
            }));
        }
        div().id("controlled-target").size_full().overflow_hidden()
            .track_focus(&self.focus)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let delta = event.delta.pixel_delta(px(16.)).y;
                let delta = f32::from(delta).round() as i32;
                this.offset = (this.offset - delta).clamp(0, 7400);
                println!("target_scroll delta_points={delta} offset_points={} phase={:?}", this.offset, event.touch_phase);
                this.invalidate(window, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, event: &MouseDownEvent, _, _| {
                println!("TARGET_RECEIVED_CLICK count={} (must NOT happen while capture tap is active)", event.click_count);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                println!("TARGET_RECEIVED_KEY {}", event.keystroke.key);
                match event.keystroke.key.as_str() {
                    "m" => this.mode = 1,
                    "b" => this.mode = 2,
                    "r" => this.mode = 3,
                    "u" => this.mode = 0,
                    "j" => this.offset = (this.offset + 1500).min(7400),
                    "q" => cx.quit(),
                    _ => {}
                }
                this.invalidate(window, cx);
            }))
            .child(img(self.image.as_ref().expect("fixture image generated").clone()).w(px(720.)).h(px(600.)))
    }
}
