//! Bounded numeric diagnostics only. Never admits, repairs, saves or exports a frame.
use crate::{fixture, stitch::Frame};
use anyhow::{Result, ensure};

#[derive(Debug, Clone, Copy)]
struct Mapping {
    base: usize,
    dx: isize,
    dy: isize,
    flipped: bool,
    swapped: bool,
}
#[derive(Debug)]
struct Score {
    mapping: Mapping,
    exact: usize,
    samples: usize,
    error: u64,
    max_delta: u8,
}

fn score(frame: &Frame, scale: usize, mapping: Mapping) -> Option<Score> {
    let mut result = Score {
        mapping,
        exact: 0,
        samples: 0,
        error: 0,
        max_delta: 0,
    };
    for y in (0..frame.height).step_by((frame.height / 32).max(1)) {
        for x in (0..frame.width).step_by((frame.width / 64).max(1)) {
            let sx = (16 * scale + x).checked_add_signed(mapping.dx)?;
            let row = if mapping.flipped {
                frame.height - 1 - y
            } else {
                y
            };
            let sy = (mapping.base + row).checked_add_signed(mapping.dy)?;
            let expected = fixture::pixel(sx, sy, scale);
            let index = (y * frame.width + x) * 4;
            let mut actual: [u8; 4] = frame.pixels[index..index + 4].try_into().ok()?;
            if mapping.swapped {
                actual.swap(0, 2);
            }
            result.samples += 1;
            result.exact += usize::from(actual == expected);
            for (a, b) in actual.into_iter().zip(expected) {
                let delta = a.abs_diff(b);
                result.error += u64::from(delta);
                result.max_delta = result.max_delta.max(delta);
            }
        }
    }
    Some(result)
}
fn best(frame: &Frame, scale: usize) -> Result<Score> {
    ensure!(
        (1..=4).contains(&scale) && frame.width > 0 && frame.height > 0,
        "invalid diagnostic geometry"
    );
    ensure!(
        frame
            .width
            .checked_mul(frame.height)
            .and_then(|n| n.checked_mul(4))
            == Some(frame.pixels.len()),
        "invalid diagnostic pixel length"
    );
    let decoded = usize::from(frame.pixels[0]) + (usize::from(frame.pixels[1]) << 8);
    let bases = [
        2040 * scale,
        decoded,
        decoded.saturating_sub(frame.height - 1),
    ];
    let mut best: Option<Score> = None;
    for base in bases {
        for flipped in [false, true] {
            for swapped in [false, true] {
                for dy in -4..=4 {
                    for dx in -4..=4 {
                        let mapping = Mapping {
                            base,
                            dx,
                            dy,
                            flipped,
                            swapped,
                        };
                        if let Some(candidate) = score(frame, scale, mapping) {
                            let rank = |s: &Score| {
                                (
                                    s.error,
                                    s.mapping.dx.unsigned_abs() + s.mapping.dy.unsigned_abs(),
                                )
                            };
                            if best.as_ref().is_none_or(|b| rank(&candidate) < rank(b)) {
                                best = Some(candidate);
                            }
                        }
                    }
                }
            }
        }
    }
    best.ok_or_else(|| anyhow::anyhow!("no diagnostic mapping"))
}
pub fn report(frame: &Frame, scale: usize) -> Result<String> {
    let selected = best(frame, scale)?;
    let decoded = usize::from(frame.pixels[0]) + (usize::from(frame.pixels[1]) << 8);
    let pixels = frame.pixels.as_chunks::<4>().0;
    let alpha_min = pixels.iter().map(|p| p[3]).min().unwrap_or(0);
    let alpha_max = pixels.iter().map(|p| p[3]).max().unwrap_or(0);
    Ok(format!(
        "assumed_initial_row={} decoded_first_row={decoded} alpha={alpha_min}..{alpha_max} best={:?} exact_samples={}/{} mean_channel_delta={:.3} max_channel_delta={}; DIAGNOSTIC ONLY",
        2040 * scale,
        selected.mapping,
        selected.exact,
        selected.samples,
        selected.error as f64 / (selected.samples * 4) as f64,
        selected.max_delta
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(flipped: bool, swapped: bool) -> Frame {
        let (width, height) = (160, 64);
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let row = if flipped { height - 1 - y } else { y };
                let mut p = fixture::pixel(16 + x, 2040 + row, 1);
                if swapped {
                    p.swap(0, 2);
                }
                pixels.extend_from_slice(&p);
            }
        }
        Frame {
            width,
            height,
            pixels,
        }
    }
    #[test]
    fn unchanged_reference_has_zero_error() {
        let frame = image(false, false);
        let result = best(&frame, 1).expect("diagnose");
        assert_eq!(result.error, 0);
        assert!(!result.mapping.flipped && !result.mapping.swapped);
        assert!(fixture::oracle(&frame, 1));
    }
    #[test]
    fn diagnoses_flip_and_swap_without_admitting_frame() {
        let frame = image(true, true);
        assert!(!fixture::oracle(&frame, 1));
        let result = best(&frame, 1).expect("diagnose");
        assert_eq!(result.error, 0);
        assert!(result.mapping.flipped && result.mapping.swapped);
        assert!(!fixture::oracle(&frame, 1));
    }
}
