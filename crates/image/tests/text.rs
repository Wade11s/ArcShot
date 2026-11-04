//! IMAGE-COMPOSE: deterministic text rasterization with a reproducible font input.
//!
//! The fixture is a fixed 80x50 logical (160x100 physical, 2x) screen. The
//! built-in font is a 5x7 monospace face; the expected patterns below are the
//! documented glyph rows scaled by the physical size, independent of any
//! runtime output.
use arcshot_core::{
    Annotation, AnnotationStyle, CapturedFrame, Document, PaletteColor, ScreenGeometry, Selection,
    TextGeometry, TextOption, TextStyle, Tool,
};
use arcshot_image::{
    BitmapFont, ComposedImage, TextFont, TextGlyphs, compose_document, compose_document_with_font,
    encode_png, measure_text, preview_bgra,
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

fn selection(x: f64, y: f64, width: f64, height: f64) -> Selection {
    Selection::try_new(x, y, width, height).unwrap()
}

/// A document with one committed text object over a uniform background.
fn text_document(
    content: &str,
    anchor: [f64; 2],
    size: f64,
    color: PaletteColor,
    background: [u8; 4],
) -> Document {
    let mut document = Document::new(frame_with(background));
    document.select(selection(0.0, 0.0, 80.0, 50.0)).unwrap();
    document.set_tool(Tool::Text);
    document
        .apply_text_option(TextOption::Stroke(color))
        .unwrap();
    document.apply_text_option(TextOption::Size(size)).unwrap();
    assert!(document.begin_text_edit(anchor));
    let metrics = measure_text(content, size);
    let [x, y, width, height] = metrics.bounds(anchor);
    document.update_text_edit(
        content,
        Selection::try_new(x, y, width, height).expect("valid measured bounds"),
    );
    assert!(document.commit_edits().unwrap());
    document
}

fn compose(document: &Document) -> ComposedImage {
    compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap()
}

fn pixel(image: &ComposedImage, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width() as usize + x as usize) * 4;
    image.rgba()[index..index + 4].try_into().unwrap()
}

/// The documented 5x7 'A' scaled to 16 pt at 2x: the top bar is 12x4 pixels.
#[test]
fn image_compose_26_text_uses_the_documented_reproducible_glyph() {
    let background = [0_u8, 0, 0, 255];
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let document = text_document("A", [10.0, 5.0], 16.0, PaletteColor::Red, background);
    let image = compose(&document);
    assert_eq!((image.width(), image.height()), (160, 100));

    // The anchor maps to physical (20,10). The 'A' top row 01110 covers source
    // columns 1..3, which scale to x=4..15 and rows 0..3.
    assert_eq!(pixel(&image, 24, 10), red, "top bar left");
    assert_eq!(pixel(&image, 35, 13), red, "top bar right");
    assert_eq!(pixel(&image, 23, 10), background, "column 0 is unset");
    assert_eq!(pixel(&image, 36, 10), background, "column 4 is unset");
    // Row 3 is 11111: the full 20-pixel-wide crossbar.
    assert_eq!(pixel(&image, 20, 22), red, "crossbar left");
    assert_eq!(pixel(&image, 39, 25), red, "crossbar right");
    assert_eq!(pixel(&image, 24, 26), background, "between the legs");
    // The baseline is at y=28, so the last row is 24..27.
    assert_eq!(pixel(&image, 20, 34), red, "leg left");
    assert_eq!(pixel(&image, 39, 37), red, "leg right");
    assert_eq!(pixel(&image, 24, 38), background, "below the glyph");

    // The same document is deterministic.
    assert_eq!(image.rgba(), compose(&document).rgba());
}

#[test]
fn image_compose_27_text_size_and_colour_apply_to_the_whole_object() {
    let background = [0_u8, 0, 0, 255];
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let blue = [0x00_u8, 0x7A, 0xFF, 255];

    // 32 pt is exactly twice 16 pt: the top bar doubles to 24x8.
    let large = text_document("A", [10.0, 5.0], 32.0, PaletteColor::Red, background);
    let image = compose(&large);
    assert_eq!(pixel(&image, 28, 10), red);
    assert_eq!(pixel(&image, 51, 17), red);
    assert_eq!(pixel(&image, 27, 10), background);
    assert_eq!(pixel(&image, 52, 10), background);
    assert_eq!(pixel(&image, 28, 18), background);

    // The colour option applies to every glyph pixel of the object.
    let colored = text_document("A", [10.0, 5.0], 16.0, PaletteColor::Blue, background);
    let image = compose(&colored);
    assert_eq!(pixel(&image, 24, 10), blue);
    assert_eq!(pixel(&image, 35, 13), blue);
    assert_eq!(pixel(&image, 23, 10), background);
}

#[test]
fn image_compose_28_text_is_clipped_to_the_selection() {
    let background = [0_u8, 0, 0, 255];
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let mut document = text_document("A", [12.0, 5.0], 16.0, PaletteColor::Red, background);
    // Resize the selection's left edge past the text anchor. The retained text
    // data does not move; the crop must drop the out-of-selection pixels.
    document.set_tool(Tool::Select);
    assert_eq!(
        document.begin_pointer([0.0, 25.0]),
        Some(arcshot_core::DragTarget::Resize(
            arcshot_core::ResizeHandle::Left
        ))
    );
    document.update_drag([20.0, 25.0]);
    assert!(document.end_drag().unwrap());
    assert_eq!(document.selection(), Some(selection(20.0, 0.0, 60.0, 50.0)));

    let image = compose(&document);
    assert_eq!((image.width(), image.height()), (120, 100));
    // The top bar of 'A' (x offset 4..15) is entirely left of the crop.
    assert_eq!(pixel(&image, 0, 10), background, "the top bar is clipped");
    // The crossbar (x offset 0..19) keeps its right end inside the crop.
    assert_eq!(pixel(&image, 0, 22), red, "the crossbar is visible");
    assert_eq!(pixel(&image, 3, 25), red);
    assert_eq!(pixel(&image, 4, 22), background);
    assert_eq!(pixel(&image, 119, 22), background);
}

#[test]
fn image_compose_29_text_layers_above_a_blur_object() {
    // A step edge makes the blur observable; a uniform surface is unchanged.
    let [width, height] = screen().pixel_size();
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..height {
        for x in 0..width {
            let color = if x >= width / 2 {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            };
            bytes.extend_from_slice(&[color[2], color[1], color[0], 255]);
        }
    }
    let frame =
        CapturedFrame::try_from_srgb_bgra(screen(), width, height, width as usize * 4, bytes)
            .unwrap();
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let mut document = Document::new(frame);
    document.select(selection(0.0, 0.0, 80.0, 50.0)).unwrap();
    // A blur rectangle below the text.
    document.set_tool(Tool::Blur);
    assert!(document.begin_pointer([10.0, 5.0]).is_some());
    document.update_drag([40.0, 25.0]);
    assert!(document.end_drag().unwrap());
    // The text object above it.
    document.set_tool(Tool::Text);
    document.apply_text_option(TextOption::Size(16.0)).unwrap();
    assert!(document.begin_text_edit([12.0, 8.0]));
    let metrics = measure_text("A", 16.0);
    let [x, y, width, height] = metrics.bounds([12.0, 8.0]);
    document.update_text_edit("A", Selection::try_new(x, y, width, height).unwrap());
    assert!(document.commit_edits().unwrap());

    let image = compose(&document);
    // The text pixels are byte-for-byte the style colour, never frosted.
    assert_eq!(pixel(&image, 28, 16), red);
    assert_eq!(pixel(&image, 39, 19), red);
    // The blurred background inside the rectangle is a blend of the step edge.
    let blurred = pixel(&image, 70, 40);
    assert_ne!(blurred, [0, 0, 0, 255]);
    assert_ne!(blurred, [255, 255, 255, 255]);
}

#[test]
fn image_compose_30_multiline_and_unknown_glyphs_are_deterministic() {
    let background = [0_u8, 0, 0, 255];
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let document = text_document("A\nB", [10.0, 5.0], 16.0, PaletteColor::Red, background);
    let image = compose(&document);
    // Line 2 baseline is 32 physical pixels below line 1.
    assert_eq!(pixel(&image, 24, 42), red, "second line top bar");
    assert_eq!(pixel(&image, 35, 45), red);
    assert_eq!(pixel(&image, 24, 41), background, "between the lines");

    // An unknown character uses the documented hollow box, deterministically.
    let unknown = text_document("中", [10.0, 5.0], 16.0, PaletteColor::Red, background);
    let image = compose(&unknown);
    assert_eq!(pixel(&image, 20, 10), red, "box top");
    assert_eq!(pixel(&image, 39, 13), red, "box top right");
    assert_eq!(pixel(&image, 24, 14), background, "box hollow");
    assert_eq!(pixel(&image, 20, 37), red, "box bottom");
}

#[test]
fn image_compose_31_the_font_input_is_pluggable() {
    /// A tiny font that renders every glyph as one opaque 2x2 block.
    struct BlockFont;
    impl TextFont for BlockFont {
        fn rasterize(&self, _content: &str, _size: f64, _scale: f64) -> Option<TextGlyphs> {
            TextGlyphs::new(2, 2, vec![255; 2 * 2 * 4])
        }
    }
    let background = [0_u8, 0, 0, 255];
    let green = [0x34_u8, 0xC7, 0x59, 255];
    let document = text_document("A", [10.0, 5.0], 16.0, PaletteColor::Green, background);
    let image = compose_document_with_font(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
        &BlockFont,
    )
    .unwrap();
    // The anchor maps to (20,10); the block covers exactly two pixels.
    assert_eq!(pixel(&image, 20, 10), green);
    assert_eq!(pixel(&image, 21, 11), green);
    assert_eq!(pixel(&image, 22, 10), background);
    assert_eq!(pixel(&image, 20, 12), background);
    // The built-in font is unchanged for the default entry point.
    let builtin = compose(&document);
    assert_eq!(pixel(&builtin, 24, 10), green);
}

#[test]
fn image_compose_32_text_preview_and_png_share_pixels() {
    let background = [0_u8, 0, 0, 255];
    let red = [0xFF_u8, 0x3B, 0x30, 255];
    let document = text_document("A", [10.0, 5.0], 16.0, PaletteColor::Red, background);
    let composed = compose(&document);
    let bgra = preview_bgra(&composed).unwrap();
    for (rgba, bgra) in composed
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bgra.as_chunks::<4>().0)
    {
        assert_eq!(bgra, &[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
    let png = encode_png(&composed).unwrap();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    buffer.truncate(info.buffer_size());
    assert_eq!(
        (info.width, info.height),
        (composed.width(), composed.height())
    );
    assert_eq!(buffer, composed.rgba());
    assert_eq!(pixel(&composed, 24, 10), red);
}

/// The font metrics agree with the rasterizer's physical box.
#[test]
fn image_compose_33_logical_metrics_match_the_physical_bitmap() {
    let content = "AB\nC";
    let size = 16.0;
    let scale = 2.0;
    let metrics = measure_text(content, size);
    assert_eq!(metrics.line_count, 2);
    assert_eq!(metrics.line_height, 16.0);
    assert_eq!(metrics.max_line_width, 2.0 * 6.0 * (size / 8.0));
    let glyphs = BitmapFont
        .rasterize(content, size, scale)
        .expect("renderable");
    let size_px = size * scale;
    let expected_height = (metrics.line_count as f64 * size_px).round() as u32;
    assert_eq!(glyphs.height(), expected_height);
    // The widest line is two characters: 2 * round(6 * size * scale / 8).
    let expected_width = (2.0 * (6.0 * size_px / 8.0).round()) as u32;
    assert_eq!(glyphs.width(), expected_width);
    // The coverage mask is deterministic and has at least one opaque pixel.
    assert!(glyphs.rgba().iter().step_by(4).any(|&alpha| alpha == 255));
}

/// A direct geometry sanity check for the text shape used by the compositor.
#[test]
fn image_compose_34_text_geometry_retains_content_and_clip_bounds() {
    let text = TextGeometry::new([3.0, 4.0], "Hi", selection(3.0, 4.0, 12.0, 16.0));
    let annotation = Annotation::text(text, TextStyle::default());
    assert_eq!(
        annotation.style(),
        AnnotationStyle::Text(TextStyle::default())
    );
    assert_eq!(
        annotation.visible_rectangle(selection(0.0, 0.0, 80.0, 50.0)),
        Some(selection(3.0, 4.0, 12.0, 16.0))
    );
    assert_eq!(
        annotation.visible_rectangle(selection(0.0, 0.0, 4.0, 4.0)),
        None
    );
    let translated = annotation.translated(2.0, 1.0).unwrap();
    assert_eq!(translated.shape().text().unwrap().anchor(), [5.0, 5.0]);
    assert_eq!(
        translated.shape().text().unwrap().bounds(),
        selection(5.0, 5.0, 12.0, 16.0)
    );
}
