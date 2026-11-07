//! IMAGE-COMPOSE: the product text path uses the macOS system sans-serif face.
//!
//! The golden 5x7 font stays the deterministic input for the image crate. These
//! tests prove the app bridge measures and rasterizes CJK and emoji through Core
//! Text, that the bitmap box matches the logical metrics, and that the committed
//! output no longer carries the hollow-box fallback.

use arcshot_app::{measure_system_text, system_font};
use arcshot_core::{
    CapturedFrame, Document, PaletteColor, ScreenGeometry, Selection, TextOption, Tool,
};
use arcshot_image::{
    BitmapFont, ComposedImage, TextFont, compose_document, compose_document_with_font,
};

fn screen() -> ScreenGeometry {
    ScreenGeometry::try_new(1, 0.0, 0.0, 80.0, 50.0, 2.0).unwrap()
}

fn frame_with(color: [u8; 4]) -> CapturedFrame {
    let [width, height] = screen().pixel_size();
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..height {
        for _ in 0..width {
            bytes.extend_from_slice(&[color[2], color[1], color[0], color[3]]);
        }
    }
    CapturedFrame::try_from_srgb_bgra(screen(), width, height, width as usize * 4, bytes).unwrap()
}

fn pixel(image: &ComposedImage, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width() as usize + x as usize) * 4;
    image.rgba()[index..index + 4].try_into().unwrap()
}

/// Renders `content` as one committed text object measured with the system face.
fn system_text_document(content: &str, size: f64) -> Document {
    let mut document = Document::new(frame_with([0, 0, 0, 255]));
    document
        .select(Selection::try_new(0.0, 0.0, 80.0, 50.0).unwrap())
        .unwrap();
    document.set_tool(Tool::Text);
    document
        .apply_text_option(TextOption::Stroke(PaletteColor::Red))
        .unwrap();
    document.apply_text_option(TextOption::Size(size)).unwrap();
    assert!(document.begin_text_edit([10.0, 5.0]));
    let metrics = measure_system_text(content, size);
    let [x, y, width, height] = metrics.bounds([10.0, 5.0]);
    document.update_text_edit(
        content,
        Selection::try_new(x, y, width.max(1.0), height.max(1.0)).expect("measured bounds"),
    );
    assert!(document.commit_edits().unwrap());
    document
}

fn compose_with_system(document: &Document) -> ComposedImage {
    compose_document_with_font(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
        system_font(),
    )
    .unwrap()
}

/// Counts pixels with any coverage inside one row of the raster.
fn row_ink(glyphs: &arcshot_image::TextGlyphs, row: u32) -> u32 {
    let width = glyphs.width() as usize;
    let start = row as usize * width * 4;
    (0..width)
        .map(|column| u32::from(glyphs.rgba()[start + column * 4 + 3] > 0))
        .sum()
}

#[test]
fn app_text_01_system_font_measures_cjk_and_emoji() {
    let latin = measure_system_text("Wi", 16.0);
    let cjk = measure_system_text("中文", 16.0);
    let emoji = measure_system_text("😀", 16.0);
    assert_eq!(cjk.line_count, 1);
    assert!(cjk.max_line_width > 0.0);
    assert!(cjk.line_height > 16.0, "font leading is part of the box");
    assert!(cjk.max_line_width > latin.max_line_width);
    assert!(emoji.max_line_width > 0.0);
    assert!(measure_system_text("AB\nC", 16.0).line_count == 2);
}

#[test]
fn app_text_02_system_raster_box_matches_measured_metrics() {
    let content = "中文\n😀";
    let size = 16.0;
    let scale = 2.0;
    let metrics = measure_system_text(content, size);
    let glyphs = system_font()
        .rasterize(content, size, scale, [0xFF, 0x3B, 0x30])
        .expect("system font renders the sample");
    let expected_width = (metrics.max_line_width * scale).ceil().max(1.0) as u32;
    let expected_height = (metrics.line_count as f64 * metrics.line_height * scale)
        .ceil()
        .max(1.0) as u32;
    assert_eq!(
        (glyphs.width(), glyphs.height()),
        (expected_width, expected_height)
    );
    // CJK and emoji produce real coverage, not an empty or all-transparent box.
    let covered = glyphs
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(covered > 20, "covered pixels: {covered}");
    // Antialiased system text has more than the two coverage values of the bitmap font.
    let distinct: std::collections::BTreeSet<u8> = glyphs
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect();
    assert!(distinct.len() > 2, "coverage levels: {}", distinct.len());
}

#[test]
fn app_text_03_system_glyphs_are_upright_and_not_the_hollow_box() {
    let size = 48.0;
    let scale = 2.0;
    let glyphs = system_font()
        .rasterize("T", size, scale, [255, 255, 255])
        .expect("system font renders T");
    let mut rows_with_ink = (0..glyphs.height()).filter(|&row| row_ink(&glyphs, row) > 0);
    let top = rows_with_ink.next().expect("top ink");
    let mut bottom = top;
    for row in rows_with_ink {
        bottom = row;
    }
    let top_bar: u32 = (top..top + 3).map(|row| row_ink(&glyphs, row)).sum();
    let stem: u32 = (bottom.saturating_sub(2)..=bottom)
        .map(|row| row_ink(&glyphs, row))
        .sum();
    assert!(
        top_bar > stem,
        "T is upright: top bar {top_bar}, stem {stem}"
    );

    let system = system_font()
        .rasterize("中", 16.0, 2.0, [255, 255, 255])
        .unwrap();
    let bitmap = BitmapFont
        .rasterize("中", 16.0, 2.0, [255, 255, 255])
        .unwrap();
    // The built-in face draws a 5x7 hollow box 24 physical pixels wide; the
    // system CJK glyph is a full-width ideograph with a different box.
    assert_eq!(bitmap.width(), 24);
    assert_ne!(system.width(), bitmap.width());
    assert_ne!(system.rgba(), bitmap.rgba());
}

#[test]
fn app_text_04_color_emoji_keeps_its_own_colours() {
    let glyphs = system_font()
        .rasterize("😀", 32.0, 2.0, [0xFF, 0x3B, 0x30])
        .expect("system font renders emoji");
    let coloured = glyphs
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 128)
        .filter(|pixel| pixel[0] != pixel[1] || pixel[1] != pixel[2])
        .count();
    assert!(coloured > 10, "colour emoji pixels: {coloured}");
}

#[test]
fn app_text_05_committed_output_uses_the_system_face() {
    let document = system_text_document("中文 Emoji 😀", 16.0);
    let system = compose_with_system(&document);
    let bitmap = compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap();
    assert_eq!((system.width(), system.height()), (160, 100));
    assert_ne!(system.rgba(), bitmap.rgba(), "the box fallback is gone");
    // The style colour reaches the composed surface.
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let red_pixels = system
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| **pixel == red)
        .count();
    assert!(red_pixels > 50, "opaque style pixels: {red_pixels}");
    // Nothing outside the measured text box was painted.
    assert_eq!(pixel(&system, 0, 0), [0, 0, 0, 255]);
}
