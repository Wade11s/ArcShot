//! IMAGE-COMPOSE: physical-pixel truth for rectangle annotations.
use arcshot_core::{
    CapturedFrame, Document, PaletteColor, PixelRect, RectangleOption, RectangleStyle,
    ScreenGeometry, Selection, StrokeWidth,
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

fn document(annotations: &[(Selection, RectangleStyle)]) -> Document {
    let mut document = Document::new(frame());
    document.select(selection(0.0, 0.0, 40.0, 20.0)).unwrap();
    for (bounds, style) in annotations {
        set_style(&mut document, *style);
        document.add_rectangle(*bounds).unwrap();
    }
    document
}

fn set_style(document: &mut Document, style: RectangleStyle) {
    for option in [
        RectangleOption::Stroke(style.stroke()),
        RectangleOption::Width(style.width()),
        RectangleOption::Fill(style.fill()),
    ] {
        document.apply_rectangle_option(option).unwrap();
    }
}

fn pixel(image: &ComposedImage, x: u32, y: u32) -> [u8; 4] {
    let index = (y as usize * image.width() as usize + x as usize) * 4;
    image.rgba()[index..index + 4].try_into().unwrap()
}

#[test]
fn image_compose_05_the_default_rectangle_draws_a_six_pixel_red_stroke_without_fill() {
    let rectangle = selection(10.0, 5.0, 10.0, 5.0);
    let document = document(&[(rectangle, RectangleStyle::default())]);
    let image = compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap();
    assert_eq!((image.width(), image.height()), (80, 40));

    // The path is (20,10)-(40,20); a 3 point stroke is 6 physical pixels,
    // centred on the path, so the top band covers rows 7..13.
    assert_eq!(pixel(&image, 20, 10), RED);
    assert_eq!(pixel(&image, 17, 7), RED);
    assert_eq!(pixel(&image, 30, 7), RED);
    assert_eq!(pixel(&image, 30, 12), RED);
    assert_eq!(
        pixel(&image, 30, 13),
        BASE,
        "stroke is only six pixels wide"
    );
    assert_eq!(pixel(&image, 30, 14), BASE, "no fill by default");
    assert_eq!(pixel(&image, 30, 16), BASE);
    assert_eq!(pixel(&image, 30, 17), RED);
    assert_eq!(pixel(&image, 30, 22), RED);
    assert_eq!(pixel(&image, 30, 23), BASE);
    assert_eq!(pixel(&image, 16, 15), BASE);
    assert_eq!(pixel(&image, 17, 15), RED);
    assert_eq!(pixel(&image, 22, 15), RED);
    assert_eq!(pixel(&image, 23, 15), BASE);
    assert_eq!(pixel(&image, 37, 15), RED);
    assert_eq!(pixel(&image, 42, 15), RED);
    assert_eq!(pixel(&image, 43, 15), BASE);
    assert_eq!(pixel(&image, 0, 0), BASE);
    assert_eq!(pixel(&image, 79, 39), BASE);
}

#[test]
fn image_compose_06_the_optional_fill_is_the_same_colour_at_twenty_percent() {
    let style = RectangleStyle::default().with_option(RectangleOption::Fill(true));
    let document = document(&[(selection(10.0, 5.0, 10.0, 5.0), style)]);
    let image = compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap();

    // (255*51 + 30*204 + 127) / 255 = 75, (59*51 + 20*204 + 127) / 255 = 28,
    // (48*51 + 10*204 + 127) / 255 = 18.
    assert_eq!(pixel(&image, 30, 15), [75, 28, 18, 255]);
    assert_eq!(pixel(&image, 30, 14), [75, 28, 18, 255]);
    assert_eq!(pixel(&image, 24, 15), [75, 28, 18, 255]);
    // The stroke itself stays fully opaque.
    assert_eq!(pixel(&image, 30, 7), RED);
    assert_eq!(pixel(&image, 30, 22), RED);
}

#[test]
fn image_compose_07_every_palette_colour_and_width_maps_to_two_x_pixels() {
    // Tall enough that the widest 5 point stroke still leaves an interior.
    let rectangle = selection(10.0, 5.0, 10.0, 15.0);
    for color in PaletteColor::ALL {
        let style = RectangleStyle::default().with_option(RectangleOption::Stroke(color));
        let document = document(&[(rectangle, style)]);
        let image = compose_document(
            document.frame(),
            document.crop().unwrap(),
            document.annotations(),
        )
        .unwrap();
        let [r, g, b] = color.rgb();
        assert_eq!(pixel(&image, 30, 7), [r, g, b, 255], "{color:?}");
        assert_eq!(pixel(&image, 30, 37), [r, g, b, 255], "{color:?}");
    }
    for (width, expected_top) in [
        (StrokeWidth::Two, 8),
        (StrokeWidth::Three, 7),
        (StrokeWidth::Five, 5),
    ] {
        let style = RectangleStyle::default().with_option(RectangleOption::Width(width));
        let document = document(&[(rectangle, style)]);
        let image = compose_document(
            document.frame(),
            document.crop().unwrap(),
            document.annotations(),
        )
        .unwrap();
        // Logical widths 2/3/5 become 4/6/10 physical pixels; the band is
        // centred on the 2x path, so it spans edge-half .. edge-half+thickness.
        let thickness = (width.points() * 2.0) as u32;
        let half = thickness / 2;
        let top_band = 10 - half;
        let bottom_band = 40 - half;
        assert_eq!(pixel(&image, 30, top_band), RED, "{width:?}");
        assert_eq!(pixel(&image, 30, top_band - 1), BASE, "{width:?}");
        assert_eq!(pixel(&image, 30, bottom_band), RED, "{width:?}");
        assert_eq!(pixel(&image, 30, bottom_band - 1), BASE, "{width:?}");
        assert_eq!(top_band, expected_top, "{width:?}");
    }
}

#[test]
fn image_compose_08_the_selection_crop_clips_annotation_pixels_at_its_edges() {
    let mut document = Document::new(frame());
    document.select(selection(5.0, 2.0, 10.0, 5.0)).unwrap();
    // The rectangle leaves the selection on every side; only the crop is output.
    document
        .add_rectangle(selection(6.0, 1.0, 8.0, 6.0))
        .unwrap();
    let crop = document.crop().unwrap();
    assert_eq!(crop, PixelRect::try_new(10, 4, 20, 10).unwrap());
    let image = compose_document(document.frame(), crop, document.annotations()).unwrap();
    assert_eq!((image.width(), image.height()), (20, 10));

    // The path is (2,-2)-(18,10) after the crop offset; the top and bottom
    // stroke bands are clipped to rows 0 and 7..10, the right band to column 19.
    assert_eq!(pixel(&image, 10, 0), RED);
    assert_eq!(pixel(&image, 10, 1), BASE);
    assert_eq!(pixel(&image, 10, 6), BASE);
    assert_eq!(pixel(&image, 10, 7), RED);
    assert_eq!(pixel(&image, 10, 9), RED);
    assert_eq!(pixel(&image, 0, 5), RED);
    assert_eq!(pixel(&image, 4, 5), RED);
    assert_eq!(pixel(&image, 5, 5), BASE, "the interior is empty");
    assert_eq!(pixel(&image, 14, 5), BASE, "the interior is empty");
    assert_eq!(pixel(&image, 15, 5), RED);
    assert_eq!(pixel(&image, 19, 5), RED);
}

#[test]
fn image_compose_09_later_objects_draw_above_earlier_ones() {
    let bounds = selection(10.0, 5.0, 10.0, 5.0);
    let lower = RectangleStyle::default().with_option(RectangleOption::Fill(true));
    let upper = RectangleStyle::new(PaletteColor::Blue, StrokeWidth::Three, true);
    let document = document(&[(bounds, lower), (bounds, upper)]);
    let image = compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap();
    // The upper blue fill wins over the lower red fill; the blue stroke wins too.
    // Semi-transparent fills stack: the blue fill composites over the red one.
    assert_eq!(pixel(&image, 30, 15), [60, 47, 65, 255]);
    assert_eq!(pixel(&image, 30, 7), [0x00, 0x7A, 0xFF, 255]);
}

#[test]
fn image_compose_10_a_crop_of_the_same_objects_matches_the_full_composition() {
    let document = document(&[
        (selection(10.0, 5.0, 10.0, 5.0), RectangleStyle::default()),
        (
            selection(15.0, 7.0, 8.0, 6.0),
            RectangleStyle::new(PaletteColor::Green, StrokeWidth::Five, true),
        ),
    ]);
    let full = compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap();
    let small_crop = PixelRect::try_new(20, 10, 20, 8).unwrap();
    let small = compose_document(document.frame(), small_crop, document.annotations()).unwrap();
    for y in 0..small.height() {
        for x in 0..small.width() {
            assert_eq!(
                pixel(&small, x, y),
                pixel(&full, x + small_crop.x(), y + small_crop.y()),
                "crop-relative pixel ({x},{y})"
            );
        }
    }
}

#[test]
fn image_compose_11_the_preview_bytes_are_the_gpu_bgra_layout_of_the_same_pixels() {
    let document = document(&[(selection(10.0, 5.0, 10.0, 5.0), RectangleStyle::default())]);
    let image = compose_document(
        document.frame(),
        document.crop().unwrap(),
        document.annotations(),
    )
    .unwrap();
    let bgra = arcshot_image::preview_bgra(&image).unwrap();
    for (rgba, bgra) in image
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bgra.as_chunks::<4>().0)
    {
        assert_eq!(bgra, &[rgba[2], rgba[1], rgba[0], rgba[3]]);
    }
}
