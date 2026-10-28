//! Deliberately strict integer-translation experiment, not a product matcher.
use anyhow::{Result, bail, ensure};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Experimental cap, NOT the V1 budget. Allows room for encode/preview/native copies.
pub const MAX_IMAGE_BYTES: usize = 96 * 1024 * 1024;

#[derive(Debug)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>, // tightly packed, opaque BGRA
}

impl Frame {
    pub fn hashes(&self) -> Vec<u64> {
        self.pixels
            .chunks_exact(self.width * 4)
            .map(|row| {
                let mut h = DefaultHasher::new();
                row.hash(&mut h);
                h.finish()
            })
            .collect()
    }

    pub fn thumbnail(&self) -> Frame {
        let scale = (180.0 / self.width as f64)
            .min(300.0 / self.height as f64)
            .min(1.0);
        let width = (self.width as f64 * scale).max(1.0) as usize;
        let height = (self.height as f64 * scale).max(1.0) as usize;
        let mut pixels = vec![0; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let from = ((y * self.height / height) * self.width + x * self.width / width) * 4;
                pixels[(y * width + x) * 4..(y * width + x + 1) * 4]
                    .copy_from_slice(&self.pixels[from..from + 4]);
            }
        }
        Frame {
            width,
            height,
            pixels,
        }
    }

    pub fn png(&self) -> Result<Vec<u8>> {
        let mut rgba = self.pixels.clone();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let mut bytes = Vec::new();
        {
            let mut encoder =
                png::Encoder::new(&mut bytes, self.width.try_into()?, self.height.try_into()?);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&rgba)?;
        }
        Ok(bytes)
    }
}

#[derive(Debug)]
pub struct Stitch {
    pub image: Frame,
    top: i64,
    last_y: i64,
    last_hashes: Vec<u64>,
    viewport_height: usize,
}

impl Stitch {
    pub fn new(frame: Frame) -> Result<Self> {
        ensure!(
            frame.width > 0 && frame.height >= 256,
            "invalid/too small viewport"
        );
        ensure!(
            frame.pixels.len() <= MAX_IMAGE_BYTES,
            "experimental image budget exceeded"
        );
        let hashes = frame.hashes();
        // Reject a viewport whose rows cannot establish a unique vertical position.
        ensure!(
            Self::candidates(&hashes, &hashes).len() == 1,
            "ambiguous/low-texture initial frame"
        );
        Ok(Self {
            viewport_height: frame.height,
            image: frame,
            top: 0,
            last_y: 0,
            last_hashes: hashes,
        })
    }

    fn candidates(previous: &[u64], next: &[u64]) -> Vec<i64> {
        let h = previous.len() as i64;
        let max = h * 3 / 4;
        (-max..=max)
            .filter(|&d| {
                let a = d.max(0) as usize;
                let b = (-d).max(0) as usize;
                let length = (h - d.abs()) as usize;
                previous[a..a + length] == next[b..b + length]
            })
            .collect()
    }

    pub fn add(&mut self, frame: Frame) -> Result<i64> {
        ensure!(
            frame.width == self.image.width && frame.height == self.viewport_height,
            "frame geometry changed"
        );
        let hashes = frame.hashes();
        let offsets = Self::candidates(&self.last_hashes, &hashes);
        let [delta] = offsets.as_slice() else {
            bail!(
                "no unique exact overlap: gap, fractional translation, mutation or ambiguity ({} candidates)",
                offsets.len()
            );
        };
        let y = self.last_y + delta;
        let bottom = self.top + self.image.height as i64;
        let overlap_top = y.max(self.top);
        let overlap_bottom = (y + frame.height as i64).min(bottom);
        ensure!(overlap_bottom > overlap_top, "no verified overlap");
        let row = frame.width * 4;
        let a = (overlap_top - self.top) as usize * row;
        let b = (overlap_top - y) as usize * row;
        let len = (overlap_bottom - overlap_top) as usize * row;
        // Hashes locate candidates only. Every overlapping pixel, including previously visited
        // content on a reversal, must agree before extending the contiguous document.
        ensure!(
            self.image.pixels[a..a + len] == frame.pixels[b..b + len],
            "previously verified content changed"
        );
        let new_top = y.min(self.top);
        let new_bottom = bottom.max(y + frame.height as i64);
        let height = usize::try_from(new_bottom - new_top)?;
        let bytes = height
            .checked_mul(row)
            .ok_or_else(|| anyhow::anyhow!("image size overflow"))?;
        ensure!(
            bytes <= MAX_IMAGE_BYTES,
            "experimental 96 MiB image cap reached"
        );
        if y < self.top {
            let prefix = (self.top - y) as usize * row;
            let mut pixels = Vec::with_capacity(bytes);
            pixels.extend_from_slice(&frame.pixels[..prefix]);
            pixels.extend_from_slice(&self.image.pixels);
            self.image.pixels = pixels;
        } else if y + frame.height as i64 > bottom {
            let start = (bottom - y) as usize * row;
            self.image
                .pixels
                .try_reserve(bytes - self.image.pixels.len())?;
            self.image.pixels.extend_from_slice(&frame.pixels[start..]);
        }
        self.image.height = height;
        self.top = new_top;
        self.last_y = y;
        self.last_hashes = hashes;
        Ok(*delta)
    }
}
