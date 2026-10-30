//! IMAGE-COMPOSE and IMAGE-LIMIT: physical crop truth and in-memory PNG decode.
use arcshot_core::{CapturedFrame, PixelRect, ScreenGeometry};
use arcshot_image::{ComposeError, ComposedImage, compose_crop, encode_png};

fn frame_with_pixels() -> CapturedFrame {
    // 5x3 stored pixels with one pixel of row padding: stride 24, width 4.
    let geometry = ScreenGeometry::try_new(1, 0.0, 0.0, 2.0, 1.5, 2.0).unwrap();
    let width = 4;
    let height = 3;
    let stride = 24;
    let mut bytes = vec![0xEE; stride * height];
    for y in 0..height {
        for x in 0..width {
            let index = y * stride + x * 4;
            bytes[index..index + 4].copy_from_slice(&[
                (x + 1) as u8,  // blue
                (y + 10) as u8, // green
                0x30,           // red
                255,
            ]);
        }
    }
    CapturedFrame::try_from_srgb_bgra(geometry, width as u32, height as u32, stride, bytes).unwrap()
}

fn decode(png: &[u8]) -> (u32, u32, png::ColorType, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    buffer.truncate(info.buffer_size());
    (info.width, info.height, info.color_type, buffer)
}

fn chunk_types(png: &[u8]) -> Vec<[u8; 4]> {
    let mut chunks = Vec::new();
    let mut offset = 8;
    while offset + 8 <= png.len() {
        let length = u32::from_be_bytes(png[offset..offset + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = png[offset + 4..offset + 8].try_into().unwrap();
        chunks.push(kind);
        offset += 12 + length;
    }
    chunks
}

#[test]
fn image_compose_01_crop_reads_only_selected_pixels_and_converts_bgra_to_rgba() {
    let frame = frame_with_pixels();
    let crop = PixelRect::try_new(1, 1, 2, 2).unwrap();
    let composed = compose_crop(&frame, crop).unwrap();
    assert_eq!((composed.width(), composed.height()), (2, 2));
    assert_eq!(
        composed.rgba(),
        &[
            0x30, 0x0B, 0x02, 0xFF, 0x30, 0x0B, 0x03, 0xFF, 0x30, 0x0C, 0x02, 0xFF, 0x30, 0x0C,
            0x03, 0xFF,
        ]
    );
}

#[test]
fn image_compose_02_crop_outside_the_frame_is_rejected_before_copying() {
    let frame = frame_with_pixels();
    assert_eq!(
        compose_crop(&frame, PixelRect::try_new(3, 0, 2, 1).unwrap()),
        Err(ComposeError::Crop)
    );
    assert_eq!(
        compose_crop(&frame, PixelRect::try_new(0, 2, 1, 2).unwrap()),
        Err(ComposeError::Crop)
    );
}

#[test]
fn image_compose_03_png_decodes_in_memory_to_the_same_rgba_srgb_pixels() {
    let frame = frame_with_pixels();
    let composed = compose_crop(&frame, PixelRect::try_new(1, 1, 2, 2).unwrap()).unwrap();
    let png = encode_png(&composed).unwrap();
    assert!(chunk_types(&png).contains(b"sRGB"));
    let (width, height, color, pixels) = decode(&png);
    assert_eq!((width, height), (2, 2));
    assert_eq!(color, png::ColorType::Rgba);
    assert_eq!(pixels, composed.rgba());
}

#[test]
fn image_limit_04_composed_surface_rejects_budget_edge_and_length_mismatch() {
    assert_eq!(
        ComposedImage::try_new(16_385, 1, Vec::new()),
        Err(ComposeError::Dimensions)
    );
    assert_eq!(
        ComposedImage::try_new(16_384, 1_466, Vec::new()),
        Err(ComposeError::ResourceLimit)
    );
    assert_eq!(
        ComposedImage::try_new(1, 1, vec![0; 3]),
        Err(ComposeError::Length)
    );
    assert!(ComposedImage::try_new(1, 1, vec![0; 4]).is_ok());
}
