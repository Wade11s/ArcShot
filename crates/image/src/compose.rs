//! Shared physical-pixel compositing and PNG encoding for approved output.
//!
//! Region capture and scroll output use this single crop/composite/encode path.
//! The input frame is immutable and never modified; no intermediate image is
//! written to disk.
use arcshot_core::{CapturedFrame, MAX_EDGE, MAX_PIXELS, MAX_PNG_BYTES, PixelRect};
use std::{error::Error, fmt};

/// A packed, opaque SDR sRGB RGBA surface produced from the immutable base.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ComposedImage {
    /// Admits an exact RGBA surface after checking dimensions and byte length.
    ///
    /// # Errors
    /// Rejects zero or oversized dimensions, budget overflow and length mismatch.
    pub fn try_new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ComposeError> {
        let length = checked_surface(width, height)?;
        if rgba.len() != length {
            return Err(ComposeError::Length);
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
    /// Physical width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Physical height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Packed opaque RGBA bytes in sRGB.
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Crops and copies the selected physical rectangle, converting BGRA to RGBA.
///
/// Row padding is skipped; only pixels inside the crop are read. Allocation is
/// reserved and checked before copying, and the crop must already satisfy the
/// document pixel and edge limits.
///
/// # Errors
/// Returns [`ComposeError::Crop`] for an out-of-frame crop and
/// [`ComposeError::ResourceLimit`] if the surface cannot be reserved.
pub fn compose_crop(frame: &CapturedFrame, crop: PixelRect) -> Result<ComposedImage, ComposeError> {
    let [frame_width, frame_height] = frame.geometry().pixel_size();
    if crop.width() == 0
        || crop.height() == 0
        || crop.x() + crop.width() > frame_width
        || crop.y() + crop.height() > frame_height
    {
        return Err(ComposeError::Crop);
    }
    let length = checked_surface(crop.width(), crop.height())?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length)
        .map_err(|_| ComposeError::ResourceLimit)?;
    let stride = frame.stride();
    for row in crop.y()..crop.y() + crop.height() {
        let start = row as usize * stride + crop.x() as usize * 4;
        let source = &frame.bgra()[start..start + crop.width() as usize * 4];
        for pixel in source.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(ComposedImage {
        width: crop.width(),
        height: crop.height(),
        rgba,
    })
}

/// Encodes a composed surface as an 8-bit RGBA PNG with an sRGB chunk.
///
/// The immutable frame and composed pixels are the only inputs; the returned
/// bytes are reused for both clipboard and explicit file output.
///
/// # Errors
/// Returns [`ComposeError::Encode`] if PNG encoding fails and
/// [`ComposeError::ResourceLimit`] when the PNG exceeds V1's byte budget.
pub fn encode_png(image: &ComposedImage) -> Result<Vec<u8>, ComposeError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header().map_err(|_| ComposeError::Encode)?;
        writer
            .write_image_data(&image.rgba)
            .map_err(|_| ComposeError::Encode)?;
        writer.finish().map_err(|_| ComposeError::Encode)?;
    }
    if bytes.len() > MAX_PNG_BYTES {
        return Err(ComposeError::ResourceLimit);
    }
    Ok(bytes)
}

fn checked_surface(width: u32, height: u32) -> Result<usize, ComposeError> {
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return Err(ComposeError::Dimensions);
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(ComposeError::Dimensions)?;
    if pixels > MAX_PIXELS {
        return Err(ComposeError::ResourceLimit);
    }
    pixels.checked_mul(4).ok_or(ComposeError::Dimensions)
}

/// Composition or encoding failed before any output side effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposeError {
    /// The crop leaves the captured frame.
    Crop,
    /// Image dimensions are empty or exceed V1's edge limit.
    Dimensions,
    /// The surface or encoded bytes exceed V1's budget.
    ResourceLimit,
    /// The supplied RGBA byte length does not match its dimensions.
    Length,
    /// PNG encoding failed.
    Encode,
}
impl fmt::Display for ComposeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Crop => "crop is outside the captured frame",
            Self::Dimensions => "image dimensions are invalid",
            Self::ResourceLimit => "image exceeds the output budget",
            Self::Length => "RGBA byte length does not match dimensions",
            Self::Encode => "PNG encoding failed",
        })
    }
}
impl Error for ComposeError {}
