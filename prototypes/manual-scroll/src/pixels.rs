//! Explicit sRGB extraction and in-memory color-path probe; never writes image files.
use anyhow::{Context, Result, ensure};
use apple_cf::cg::{CGContext, CGImage};
use screencapturekit::screenshot_manager::CGImageExt;

pub fn srgb_bgra(image: &CGImage) -> Result<Vec<u8>> {
    let (width, height) = (image.width(), image.height());
    let len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .context("image size overflow")?;
    ensure!(
        len > 0 && len <= crate::stitch::MAX_IMAGE_BYTES,
        "image exceeds extraction budget"
    );
    let context = CGContext::new_srgb_rgba8(width, height)?;
    ensure!(
        context.bytes_per_row() == width * 4,
        "unexpected sRGB row stride"
    );
    context.draw_image(0., 0., width as f64, height as f64, image);
    // SAFETY: Fresh uniquely owned, initialized bitmap context. Drawing above has
    // returned; no native call or retained alias mutates it during this owned copy.
    let mut bytes = unsafe { context.as_bytes() }.to_vec();
    ensure!(bytes.len() == len, "unexpected sRGB extraction length");
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(bytes)
}

fn reference_image() -> Result<(CGImage, Vec<u8>)> {
    let (width, height) = (160, 32);
    let mut source = CGContext::new_srgb_rgba8(width, height)?;
    let mut expected = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            expected.extend_from_slice(&crate::fixture::pixel(16 + x, 2016 + y, 1));
        }
    }
    // SAFETY: Fresh uniquely owned bitmap, initialized by the binding. No snapshot
    // or retained alias exists yet; no native operation accesses it during the write.
    let bytes = unsafe { source.as_bytes_mut() };
    for (rgba, bgra) in bytes
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(expected.as_chunks::<4>().0)
    {
        *rgba = [bgra[2], bgra[1], bgra[0], bgra[3]];
    }
    let image = source
        .snapshot_to_image()
        .context("cannot snapshot reference bitmap")?;
    Ok((image, expected))
}

pub fn probe() -> Result<()> {
    let (image, expected) = reference_image()?;
    let old = image.bgra_data()?;
    let normalized = srgb_bgra(&image)?;
    let count = |bytes: &[u8]| {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .filter(|(a, b)| *a == *b)
            .count()
    };
    println!(
        "in_memory_color_probe pixels={} DeviceRGB_exact={} named_sRGB_exact={}",
        expected.len() / 4,
        count(&old),
        count(&normalized)
    );
    ensure!(
        normalized == expected,
        "sRGB bitmap round-trip changed reference pixels"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srgb_normalization_preserves_opaque_fixture_colors_and_row_order() {
        let (image, expected) = reference_image().expect("create reference");
        assert_eq!(srgb_bgra(&image).expect("normalize"), expected);
    }
}
