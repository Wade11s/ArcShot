//! IMAGE-COMPOSE: physical-pixel truth for line and arrow annotations.
use arcshot_core::{
    ArrowOption, ArrowStyle, CapturedFrame, Document, LineOption, LineStyle, PaletteColor,
    PixelRect, ScreenGeometry, Selection, StrokeWidth, Tool,
};
use arcshot_image::{ComposedImage, compose_document};

const BASE: [u8; 4] = [30, 20, 10, 255];
const RED: [u8; 4] = [0xFF, 0x3B, 0x30, 255];

fn screen() -> ScreenGeometry {
    // 40x20 logical points at 2x: 80x40 physical pixels.
    ScreenGeometry::try_new(1, 0.0, 0.0, 40.0, 20.0, 2.0).unwrap()
}

fn frame() -> CapturedFrame {
    let [w, h] = screen().pixel_size();
    let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
    for _ in 0..w as usize * h as usize {
        // Uniform opaque BGRA base: stored blue=10, green=20, red=30.
        bytes.extend_from_slice(&[10, 20, 30, 255]);
    }
    CapturedFrame::try_from_srgb_bgra(screen(), w, h, w as usize * 4, bytes).unwrap()
}

fn selection(x: f64, y: f64, width: f64, height: f64) -> Selection {
    Selection::try_new(x, y, width, height).unwrap()
}

fn document() -> Document {
    let mut document = Document::new(frame());
    document.select(selection(0.0, 0.0, 40.0, 20.0)).unwrap();
    document
}

fn line(style: LineStyle, from: [f64; 2], to: [f64; 2]) -> Document {
    let mut document = document();
    document.set_tool(Tool::Line);
    document.deselect();
    for option in [
        LineOption::Stroke(style.stroke()),
        LineOption::Width(style.width()),
        LineOption::Dash(style.dash()),
    ] {
        document.apply_line_option(option).unwrap();
    }
    assert!(document.begin_pointer(from).is_some());
    document.update_drag(to);
    assert!(document.end_drag().unwrap());
    document
}

fn arrow(style: ArrowStyle, from: [f64; 2], to: [f64; 2]) -> Document {
    let mut document = document();
    document.set_tool(Tool::Arrow);
    document.deselect();
    for option in [
        ArrowOption::Stroke(style.stroke()),
        ArrowOption::Width(style.width()),
        ArrowOption::Inward(style.inward()),
    ] {
        document.apply_arrow_option(option).unwrap();
    }
    assert!(document.begin_pointer(from).is_some());
    document.update_drag(to);
    assert!(document.end_drag().unwrap());
    document
}

fn compose(document: &Document, crop: PixelRect) -> ComposedImage {
    compose_document(document.frame(), crop, document.annotations()).unwrap()
}

fn full(document: &Document) -> ComposedImage {
    compose(document, document.crop().unwrap())
}

fn pixel(image: &ComposedImage, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width() as usize + x as usize) * 4;
    image.rgba()[index..index + 4].try_into().unwrap()
}

/// Min and max rows painted with `color` in one column.
fn column_span(image: &ComposedImage, x: u32, color: [u8; 4]) -> Option<(u32, u32)> {
    let mut span: Option<(u32, u32)> = None;
    for y in 0..image.height() {
        if pixel(image, x, y) == color {
            span = Some(match span {
                Some((top, _)) => (top, y),
                None => (y, y),
            });
        }
    }
    span
}

/// Number of rows painted with `color` in one column.
fn column_count(image: &ComposedImage, x: u32, color: [u8; 4]) -> u32 {
    (0..image.height())
        .filter(|&y| pixel(image, x, y) == color)
        .count() as u32
}

/// Min and max columns painted with `color` in one row.
fn row_span(image: &ComposedImage, y: u32, color: [u8; 4]) -> Option<(u32, u32)> {
    let mut span: Option<(u32, u32)> = None;
    for x in 0..image.width() {
        if pixel(image, x, y) == color {
            span = Some(match span {
                Some((left, _)) => (left, x),
                None => (x, x),
            });
        }
    }
    span
}

/// Contiguous painted row runs in one column.
fn column_bands(image: &ComposedImage, x: u32, color: [u8; 4]) -> Vec<(u32, u32)> {
    let mut bands: Vec<(u32, u32)> = Vec::new();
    for y in 0..image.height() {
        if pixel(image, x, y) != color {
            continue;
        }
        match bands.last_mut() {
            Some(last) if last.1 + 1 == y => last.1 = y,
            _ => bands.push((y, y)),
        }
    }
    bands
}

#[test]
fn image_compose_12_solid_lines_use_the_stroke_width_in_physical_pixels() {
    for width in StrokeWidth::ALL {
        let document = line(
            LineStyle::default().with_option(LineOption::Width(width)),
            [10.0, 10.0],
            [30.0, 10.0],
        );
        let image = full(&document);
        // The 2x path is (20,20)-(60,20); a 2/3/5 point stroke is 4/6/10 pixels.
        let thickness = (width.points() * 2.0) as u32;
        let half = thickness / 2;
        assert_eq!(row_span(&image, 20, RED), Some((20, 59)), "{width:?}");
        assert_eq!(
            column_span(&image, 40, RED),
            Some((20 - half, 20 + half - 1)),
            "{width:?}"
        );
        assert_eq!(pixel(&image, 40, 19 - half), BASE, "{width:?}");
        assert_eq!(pixel(&image, 40, 20 + half), BASE, "{width:?}");
        assert_eq!(pixel(&image, 19, 20), BASE, "flat butt cap before start");
        assert_eq!(pixel(&image, 60, 20), BASE, "flat butt cap after end");
        assert_eq!(pixel(&image, 40, 20), RED, "{width:?}");
    }
}

#[test]
fn image_compose_13_the_dash_pattern_scales_with_the_stroke_width() {
    // Two point: 3x2 = 12 painted pixels, then a 1.5x2 = 6 pixel gap.
    let thin = line(
        LineStyle::default()
            .with_option(LineOption::Width(StrokeWidth::Two))
            .with_option(LineOption::Dash(true)),
        [5.0, 10.0],
        [35.0, 10.0],
    );
    let image = full(&thin);
    assert_eq!(row_span(&image, 20, RED), Some((10, 69)));
    for x in 10..=21 {
        assert_eq!(pixel(&image, x, 20), RED, "first dash column {x}");
    }
    for x in 22..=27 {
        assert_eq!(pixel(&image, x, 20), BASE, "six pixel gap column {x}");
    }
    for x in 28..=39 {
        assert_eq!(pixel(&image, x, 20), RED, "second dash column {x}");
    }
    for x in 40..=45 {
        assert_eq!(pixel(&image, x, 20), BASE, "second gap column {x}");
    }

    // Three point: 3x3 = 9 logical = 18 painted pixels, then a 9 pixel gap.
    let medium = line(
        LineStyle::default()
            .with_option(LineOption::Dash(true))
            .with_option(LineOption::Width(StrokeWidth::Three)),
        [5.0, 10.0],
        [35.0, 10.0],
    );
    let image = full(&medium);
    for x in 10..=27 {
        assert_eq!(pixel(&image, x, 20), RED, "first dash column {x}");
    }
    for x in 28..=36 {
        assert_eq!(pixel(&image, x, 20), BASE, "nine pixel gap column {x}");
    }
    for x in 37..=54 {
        assert_eq!(pixel(&image, x, 20), RED, "second dash column {x}");
    }

    // Five point: 3x5 = 15 logical = 30 painted pixels, then a 15 pixel gap.
    let thick = line(
        LineStyle::default()
            .with_option(LineOption::Width(StrokeWidth::Five))
            .with_option(LineOption::Dash(true)),
        [5.0, 10.0],
        [35.0, 10.0],
    );
    let image = full(&thick);
    for x in 10..=39 {
        assert_eq!(pixel(&image, x, 20), RED, "first dash column {x}");
    }
    for x in 40..=54 {
        assert_eq!(pixel(&image, x, 20), BASE, "fifteen pixel gap column {x}");
    }
    for x in 55..=69 {
        assert_eq!(pixel(&image, x, 20), RED, "second dash column {x}");
    }
}

#[test]
fn image_compose_14_flat_arrows_keep_a_constant_body_and_a_filled_triangle_head() {
    let document = arrow(ArrowStyle::default(), [5.0, 10.0], [35.0, 10.0]);
    let image = full(&document);
    // The 2x arrow runs tail x=10 to tip x=70. A three point stroke gives a
    // six pixel body, an 18 pixel head and a 9.6 pixel head half width.
    assert_eq!(column_span(&image, 10, RED), Some((17, 22)), "flat tail");
    assert_eq!(pixel(&image, 9, 20), BASE);
    for x in [11, 30, 51] {
        assert_eq!(column_count(&image, x, RED), 6, "constant body at {x}");
    }
    let (top, bottom) = column_span(&image, 52, RED).expect("head base is painted");
    assert!(
        bottom - top + 1 >= 18,
        "the head is wider than the body: {}",
        bottom - top + 1
    );
    let narrow = column_count(&image, 60, RED);
    let tip = column_count(&image, 68, RED);
    assert!(narrow < bottom - top + 1, "head narrows toward the tip");
    assert!(tip < narrow, "the tip is the narrowest head column");
    assert!(tip > 0, "the filled tip still covers pixels");
    assert_eq!(pixel(&image, 69, 20), BASE, "the very tip is sub-pixel");
    assert_eq!(pixel(&image, 40, 16), BASE);
    assert_eq!(pixel(&image, 52, 9), BASE, "outside the head barb");
    assert_eq!(pixel(&image, 52, 11), RED, "inside the head barb");
    assert_eq!(
        column_bands(&image, 55, RED).len(),
        1,
        "a flat arrow stays solid behind its head"
    );
}

#[test]
fn image_compose_15_inward_arrows_taper_from_a_thin_tail_into_a_concave_head() {
    let document = arrow(
        ArrowStyle::default().with_option(ArrowOption::Inward(true)),
        [5.0, 10.0],
        [35.0, 10.0],
    );
    let image = full(&document);
    // Tail x=10, tip x=70. The tapering body widens from the thin tail to the
    // notch at x=59.2; the barbs at x=52 stick out behind that notch.
    assert_eq!(column_count(&image, 10, RED), 2, "thin tail");
    let tail = column_count(&image, 20, RED);
    let middle = column_count(&image, 40, RED);
    let before_barb = column_count(&image, 50, RED);
    assert!(tail < middle, "the body widens from the tail");
    assert!(
        middle < before_barb,
        "the body keeps widening toward the head"
    );
    assert!(tail < 6, "the inward tail is thinner than a flat body");

    // Behind the notch the concave rear edge splits the silhhouette into the
    // two barbs and the shaft; a double-headed arrow would not have this gap.
    let bands = column_bands(&image, 53, RED);
    assert_eq!(bands.len(), 3, "concave rear edge bands: {bands:?}");
    let (barb_top, barb_bottom) = (bands[0].0, bands[2].1);
    let (shaft_top, shaft_bottom) = bands[1];
    assert!(barb_top < shaft_top && shaft_bottom < barb_bottom);
    let spread = column_span(&image, 53, RED).unwrap();
    let shaft_only = column_span(&image, 50, RED).unwrap();
    assert!(
        spread.1 - spread.0 > shaft_only.1 - shaft_only.0 + 8,
        "the barbs extend past the shaft: {spread:?} vs {shaft_only:?}"
    );
    // At and in front of the notch the head is one solid region.
    assert_eq!(column_bands(&image, 50, RED).len(), 1);
    assert_eq!(column_bands(&image, 60, RED).len(), 1);
    let head = column_count(&image, 60, RED);
    let tip = column_count(&image, 68, RED);
    assert!(head > tip, "the head narrows toward the tip");
    assert!(tip > 0);
    // No second head at the tail: only the head side has the wide barbs.
    assert_eq!(column_bands(&image, 12, RED).len(), 1);
    assert!(column_count(&image, 12, RED) < 6);

    let reversed = arrow(
        ArrowStyle::default().with_option(ArrowOption::Inward(true)),
        [35.0, 10.0],
        [5.0, 10.0],
    );
    let image = full(&reversed);
    assert_eq!(
        column_bands(&image, 25, RED).len(),
        3,
        "the concave head moved to the start side"
    );
    assert_eq!(column_bands(&image, 17, RED).len(), 1);
    assert!(column_count(&image, 67, RED) < 6, "thin tail near x=70");
}

#[test]
fn image_compose_16_lines_and_arrows_clip_to_the_selection_crop() {
    let mut document = Document::new(frame());
    document.select(selection(5.0, 2.0, 10.0, 5.0)).unwrap();
    document.set_tool(Tool::Line);
    assert!(document.begin_pointer([5.0, 4.0]).is_some());
    document.update_drag([18.0, 4.0]);
    document.end_drag().unwrap();
    document.set_tool(Tool::Arrow);
    assert!(document.begin_pointer([6.0, 3.0]).is_some());
    document.update_drag([17.0, 6.0]);
    document.end_drag().unwrap();

    let crop = document.crop().unwrap();
    assert_eq!(crop, PixelRect::try_new(10, 4, 20, 10).unwrap());
    let image = compose(&document, crop);
    assert_eq!((image.width(), image.height()), (20, 10));
    // The horizontal line enters at the left crop edge and leaves at the right;
    // the crop keeps only its three pixel band relative to the crop origin.
    for x in [0, 10, 19] {
        assert_eq!(pixel(&image, x, 3), RED, "line band at crop column {x}");
    }
    assert_eq!(pixel(&image, 0, 0), BASE);
    assert_eq!(pixel(&image, 0, 9), BASE);
    assert_eq!(pixel(&image, 19, 5), RED);
}

#[test]
fn image_compose_17_any_crop_composes_the_same_line_and_arrow_pixels() {
    let mut document = document();
    document.set_tool(Tool::Line);
    assert!(document.begin_pointer([3.0, 15.0]).is_some());
    document.update_drag([36.0, 6.0]);
    document.end_drag().unwrap();
    document.set_tool(Tool::Arrow);
    assert!(document.begin_pointer([6.0, 5.0]).is_some());
    document.update_drag([34.0, 16.0]);
    document.end_drag().unwrap();

    let whole = full(&document);
    let small_crop = PixelRect::try_new(18, 8, 26, 20).unwrap();
    let small = compose(&document, small_crop);
    for y in 0..small.height() {
        for x in 0..small.width() {
            assert_eq!(
                pixel(&small, x, y),
                pixel(&whole, x + small_crop.x(), y + small_crop.y()),
                "crop-relative pixel ({x},{y})"
            );
        }
    }
    // The palette colour is the fixed opaque sRGB red.
    let blue = line(
        LineStyle::default().with_option(LineOption::Stroke(PaletteColor::Blue)),
        [10.0, 10.0],
        [30.0, 10.0],
    );
    let image = full(&blue);
    assert_eq!(pixel(&image, 40, 20), [0x00, 0x7A, 0xFF, 255]);
}

#[test]
fn image_compose_18_both_arrow_heads_stay_distinguishable_at_every_width() {
    for width in StrokeWidth::ALL {
        let points = width.points();
        // With a 30 point arrow the head is 3x the stroke width and the notch
        // sits 60% of that behind the tip. Sample between the barbs and notch.
        let head = 3.0 * points * 2.0;
        let notch = head * 0.6;
        let column = (70.0 - (head + notch) / 2.0) as u32;
        let flat = arrow(
            ArrowStyle::default().with_option(ArrowOption::Width(width)),
            [5.0, 10.0],
            [35.0, 10.0],
        );
        let flat = full(&flat);
        let inward = arrow(
            ArrowStyle::default()
                .with_option(ArrowOption::Width(width))
                .with_option(ArrowOption::Inward(true)),
            [5.0, 10.0],
            [35.0, 10.0],
        );
        let inward = full(&inward);

        assert_eq!(
            column_bands(&flat, column, RED).len(),
            1,
            "flat arrow stays solid at column {column} for {width:?}"
        );
        assert_eq!(
            column_bands(&inward, column, RED).len(),
            3,
            "inward arrow shows the notch at column {column} for {width:?}"
        );
        // The flat body is exactly one stroke wide; the inward tail is thinner.
        let body = (points * 2.0) as u32;
        assert_eq!(column_count(&flat, 30, RED), body, "flat body at {width:?}");
        assert!(
            column_count(&inward, 12, RED) < body,
            "inward tail at {width:?}"
        );
    }
}
