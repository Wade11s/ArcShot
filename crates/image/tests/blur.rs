//! IMAGE-BLUR: physical-pixel truth for the frosted Blur Rectangle.
//!
//! The fixture is a fixed 80x50 logical (160x100 physical, 2x) screen. Every
//! expected value below is a worked example of the documented gaussian: the
//! kernel support is exactly the physical radius (4/8/12 points at 2x), sigma
//! is half the radius, and the separable passes round to the nearest u8.
use arcshot_core::{
    BlurLevel, BlurOption, CapturedFrame, Document, PaletteColor, RectangleOption, ScreenGeometry,
    Selection, StrokeWidth, Tool,
};
use arcshot_image::{ComposedImage, compose_document, encode_png, preview_bgra};

const EDGE: u32 = 80;

fn screen() -> ScreenGeometry {
    ScreenGeometry::try_new(1, 0.0, 0.0, 80.0, 50.0, 2.0).unwrap()
}

/// Builds an opaque BGRA frame from one RGBA value per physical pixel.
fn frame_with(pixels: impl Fn(u32, u32) -> [u8; 4]) -> CapturedFrame {
    let [width, height] = screen().pixel_size();
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let [red, green, blue, alpha] = pixels(x, y);
            bytes.extend_from_slice(&[blue, green, red, alpha]);
        }
    }
    CapturedFrame::try_from_srgb_bgra(screen(), width, height, width as usize * 4, bytes).unwrap()
}

fn step_edge(x_edge: u32) -> CapturedFrame {
    frame_with(move |x, _| {
        if x >= x_edge {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    })
}

/// A white column at x=0 on black: the selection edge is the pattern edge.
fn edge_column() -> CapturedFrame {
    frame_with(move |x, _| {
        if x == 0 {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    })
}

/// A white block from x=20 to x=60 on black.
fn block() -> CapturedFrame {
    frame_with(move |x, _| {
        if (20..60).contains(&x) {
            [255, 255, 255, 255]
        } else {
            [0, 0, 0, 255]
        }
    })
}

fn selection(x: f64, y: f64, width: f64, height: f64) -> Selection {
    Selection::try_new(x, y, width, height).unwrap()
}

fn document(frame: CapturedFrame) -> Document {
    let mut document = Document::new(frame);
    document.select(selection(0.0, 0.0, 80.0, 50.0)).unwrap();
    document
}

/// Adds one blur rectangle above the current layers with the given level.
fn add_blur(document: &mut Document, level: BlurLevel, bounds: Selection) {
    document.set_tool(Tool::Blur);
    document.deselect();
    document
        .apply_blur_option(BlurOption::Level(level))
        .unwrap();
    assert!(document.begin_pointer([bounds.x(), bounds.y()]).is_some());
    document.update_drag([bounds.x() + bounds.width(), bounds.y() + bounds.height()]);
    assert!(document.end_drag().unwrap());
}

fn blur_document(level: BlurLevel, frame: CapturedFrame, bounds: Selection) -> Document {
    let mut document = document(frame);
    add_blur(&mut document, level, bounds);
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

/// First column whose value reaches `threshold`, scanning from the edge outward.
fn first_reaching(image: &ComposedImage, threshold: u8) -> u32 {
    for x in 0..image.width() {
        if pixel(image, x, 50)[0] >= threshold {
            return x;
        }
    }
    image.width()
}

#[test]
fn image_blur_19_three_levels_use_the_specified_physical_radii() {
    // The step edge is constant along y, so the row profile is exactly the
    // horizontal gaussian pass. The support reaches exactly +-radius around
    // the edge and the 10-90 width grows with the level.
    let cases = [
        (BlurLevel::Weak, 8_u32, 4_u8, 141_u8, 10_u32),
        (BlurLevel::Medium, 16, 2, 134, 20),
        (BlurLevel::Strong, 24, 1, 132, 28),
    ];
    for (level, radius, radius_value, edge_value, width) in cases {
        let document = blur_document(level, step_edge(EDGE), selection(0.0, 0.0, 80.0, 50.0));
        let image = compose(&document);
        assert_eq!(image.height(), 100, "{level:?}");
        // One pixel before the support is untouched black.
        assert_eq!(pixel(&image, EDGE - radius - 1, 50)[0], 0, "{level:?}");
        // The support ends exactly at the physical radius.
        assert_eq!(
            pixel(&image, EDGE - radius, 50)[0],
            radius_value,
            "{level:?}"
        );
        // The edge centre and the far side of the support.
        assert_eq!(pixel(&image, EDGE, 50)[0], edge_value, "{level:?}");
        assert_eq!(pixel(&image, EDGE + radius, 50)[0], 255, "{level:?}");
        assert_eq!(pixel(&image, EDGE + radius + 1, 50)[0], 255, "{level:?}");
        // 10% and 90% of full scale: the transition widens with the radius.
        let low = first_reaching(&image, 26);
        let high = first_reaching(&image, 230);
        assert_eq!(high - low, width, "{level:?}");
        // The transition is smooth, never a hard block mosaic.
        let distinct = (low..=high)
            .map(|x| pixel(&image, x, 50)[0])
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert!(distinct >= width as usize, "{level:?}");
    }
}

#[test]
fn image_blur_20_a_uniform_surface_is_unchanged_with_no_colour_or_alpha_mask() {
    let base = [10_u8, 20, 30, 255];
    for level in BlurLevel::ALL {
        let document = blur_document(
            level,
            frame_with(|_, _| base),
            selection(0.0, 0.0, 80.0, 50.0),
        );
        let image = compose(&document);
        for y in 0..image.height() {
            for x in 0..image.width() {
                assert_eq!(pixel(&image, x, y), base, "{level:?} at ({x},{y})");
            }
        }
    }
}

#[test]
fn image_blur_21_blur_is_clipped_to_the_rectangle() {
    // The rectangle covers x=0..80; the step edge is exactly at its right
    // boundary. Sampling may leave the rectangle but the output may not.
    for (level, inside) in [
        (BlurLevel::Weak, 114_u8),
        (BlurLevel::Medium, 121),
        (BlurLevel::Strong, 123),
    ] {
        let document = blur_document(level, step_edge(EDGE), selection(0.0, 0.0, 40.0, 50.0));
        let image = compose(&document);
        assert_eq!(pixel(&image, 79, 50)[0], inside, "{level:?}");
        assert_eq!(pixel(&image, 80, 50)[0], 255, "{level:?}");
        assert_eq!(pixel(&image, 0, 50)[0], 0, "{level:?}");
        assert_eq!(pixel(&image, 159, 50)[0], 255, "{level:?}");
    }
}

#[test]
fn image_blur_22_the_selection_edge_extends_its_edge_pixels() {
    // The only white pixel sits on the selection boundary. Clamping the
    // sampling coordinate repeats that edge pixel, so the boundary value is
    // the half-kernel sum, not the tiny isolated-pixel response.
    let cases = [
        (BlurLevel::Weak, 141_u8, 114_u8),
        (BlurLevel::Medium, 134, 121),
        (BlurLevel::Strong, 132, 123),
    ];
    for (level, boundary, next) in cases {
        let document = blur_document(level, edge_column(), selection(0.0, 0.0, 80.0, 50.0));
        let image = compose(&document);
        assert_eq!(pixel(&image, 0, 50)[0], boundary, "{level:?}");
        assert_eq!(pixel(&image, 1, 50)[0], next, "{level:?}");
        // The effect never enters the black side beyond the support.
        let radius = (level.points() * 2.0) as u32;
        assert_eq!(
            pixel(&image, radius + 2, 50)[0],
            0,
            "{level:?} stops at the radius"
        );
    }
}

#[test]
fn image_blur_23_lower_layers_blur_and_later_objects_stay_crisp() {
    let blur_bounds = selection(30.0, 0.0, 20.0, 50.0);
    let mut with_blur = document(step_edge(EDGE));
    // Lower annotation: a filled red rectangle below the blur.
    with_blur.deselect();
    for option in [
        RectangleOption::Stroke(PaletteColor::Red),
        RectangleOption::Width(StrokeWidth::Three),
        RectangleOption::Fill(true),
    ] {
        with_blur.apply_rectangle_option(option).unwrap();
    }
    with_blur
        .add_rectangle(selection(30.0, 0.0, 40.0, 50.0))
        .unwrap();
    add_blur(&mut with_blur, BlurLevel::Medium, blur_bounds);
    // Later annotation: a solid blue stroke rectangle above the blur.
    with_blur.deselect();
    for option in [
        RectangleOption::Stroke(PaletteColor::Blue),
        RectangleOption::Width(StrokeWidth::Three),
        RectangleOption::Fill(false),
    ] {
        with_blur.apply_rectangle_option(option).unwrap();
    }
    with_blur
        .add_rectangle(selection(45.0, 10.0, 5.0, 30.0))
        .unwrap();

    // The same document without the blur is the reference for what the lower
    // layer would look like if the frosted rectangle were absent.
    let mut without_blur = document(step_edge(EDGE));
    without_blur.deselect();
    for option in [
        RectangleOption::Stroke(PaletteColor::Red),
        RectangleOption::Width(StrokeWidth::Three),
        RectangleOption::Fill(true),
    ] {
        without_blur.apply_rectangle_option(option).unwrap();
    }
    without_blur
        .add_rectangle(selection(30.0, 0.0, 40.0, 50.0))
        .unwrap();
    without_blur.deselect();
    for option in [
        RectangleOption::Stroke(PaletteColor::Blue),
        RectangleOption::Width(StrokeWidth::Three),
        RectangleOption::Fill(false),
    ] {
        without_blur.apply_rectangle_option(option).unwrap();
    }
    without_blur
        .add_rectangle(selection(45.0, 10.0, 5.0, 30.0))
        .unwrap();

    let blurred = compose(&with_blur);
    let reference = compose(&without_blur);
    // A lower-layer pixel inside the rectangle changes under the blur.
    assert_ne!(pixel(&blurred, 70, 50), pixel(&reference, 70, 50));
    // The later blue stroke is byte-for-byte the same with and without it.
    let blue = [0x00_u8, 0x7A, 0xFF, 255];
    assert_eq!(pixel(&blurred, 95, 20), blue);
    assert_eq!(pixel(&reference, 95, 20), blue);
    // Pixels outside the rectangle are untouched by the blur.
    assert_eq!(pixel(&blurred, 10, 50), pixel(&reference, 10, 50));
    assert_eq!(pixel(&blurred, 150, 50), pixel(&reference, 150, 50));
}

#[test]
fn image_blur_24_moving_or_resizing_recomputes_at_the_new_geometry() {
    let mut document = document(block());
    add_blur(
        &mut document,
        BlurLevel::Medium,
        selection(10.0, 0.0, 10.0, 50.0),
    );
    let before = compose(&document);
    assert!(pixel(&before, 30, 50)[0] < 255, "the block is frosted");
    assert_eq!(pixel(&before, 50, 50)[0], 255, "outside the rectangle");

    // Enlarge the rectangle to cover the rest of the block.
    document.set_tool(Tool::Select);
    assert!(document.begin_pointer([20.0, 50.0]).is_some());
    document.update_drag([30.0, 50.0]);
    assert!(document.end_drag().unwrap());
    let resized = compose(&document);
    assert!(pixel(&resized, 50, 50)[0] < 255, "the resize recomputes");

    // Move the rectangle away from the block; the block becomes sharp again.
    assert!(document.begin_pointer([20.0, 25.0]).is_some());
    document.update_drag([50.0, 25.0]);
    assert!(document.end_drag().unwrap());
    let moved = compose(&document);
    assert_eq!(pixel(&moved, 30, 50)[0], 255, "the frosting moved away");
    assert_eq!(pixel(&moved, 50, 50)[0], 255);
}

#[test]
fn image_blur_25_the_effect_is_deterministic_and_preview_png_share_pixels() {
    let document = blur_document(
        BlurLevel::Strong,
        step_edge(EDGE),
        selection(10.0, 5.0, 40.0, 30.0),
    );
    let first = compose(&document);
    let second = compose(&document);
    assert_eq!(
        first.rgba(),
        second.rgba(),
        "the same document is deterministic"
    );
    let full = first;

    // Preview bytes and PNG output are derived from the same composed pixels.
    let bgra = preview_bgra(&full).unwrap();
    for (rgba, bgra) in full
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bgra.as_chunks::<4>().0)
    {
        assert_eq!(bgra, &[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
    let png = encode_png(&full).unwrap();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    buffer.truncate(info.buffer_size());
    assert_eq!((info.width, info.height), (full.width(), full.height()));
    assert_eq!(buffer, full.rgba());
}
