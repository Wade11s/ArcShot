//! Fixture-only coordinates. Measure once during preparation, never retarget mid-capture.
use crate::native::{Point, Rect, Size};
use anyhow::{Result, ensure};

#[derive(Clone, Copy)]
pub enum PreviewPlacement {
    Right,
    Left,
    Inside,
}

pub fn fixture_selection(frame: Rect) -> Result<Rect> {
    ensure!(
        frame.origin.x.is_finite() && frame.origin.y.is_finite(),
        "invalid target origin"
    );
    ensure!(
        frame.size.width == 720.0 && frame.size.height == 600.0,
        "target is not the full-sized fixture"
    );
    Ok(Rect {
        origin: Point {
            x: frame.origin.x + 16.0,
            y: frame.origin.y + 16.0,
        },
        size: Size {
            width: 680.0,
            height: 540.0,
        },
    })
}

pub fn preview_frame(selection: Rect, screen: Rect, placement: PreviewPlacement) -> Result<Rect> {
    let right = selection.origin.x + selection.size.width;
    let screen_right = screen.origin.x + screen.size.width;
    let width = 240.0;
    let gap = 8.0;
    let left_fits = selection.origin.x - gap - width >= screen.origin.x;
    let right_fits = right + gap + width <= screen_right;
    let outside = match placement {
        PreviewPlacement::Inside => None,
        PreviewPlacement::Right if right_fits => Some(right + gap),
        PreviewPlacement::Left if left_fits => Some(selection.origin.x - gap - width),
        _ if right_fits => Some(right + gap),
        _ if left_fits => Some(selection.origin.x - gap - width),
        _ => None,
    };
    let height = if outside.is_some() { 560.0 } else { 460.0 };
    ensure!(
        screen.size.width >= width && screen.size.height >= height,
        "preview does not fit display"
    );
    let x = outside.unwrap_or(right - width);
    let y = if outside.is_some() {
        selection.origin.y
    } else {
        selection.origin.y + selection.size.height - height
    };
    Ok(Rect {
        origin: Point {
            x: x.clamp(screen.origin.x, screen_right - width),
            y: y.clamp(
                screen.origin.y,
                screen.origin.y + screen.size.height - height,
            ),
        },
        size: Size { width, height },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f64, y: f64) -> Rect {
        Rect {
            origin: Point { x, y },
            size: Size {
                width: 720.,
                height: 600.,
            },
        }
    }
    fn screen() -> Rect {
        Rect {
            origin: Point { x: 0., y: 0. },
            size: Size {
                width: 1470.,
                height: 956.,
            },
        }
    }
    #[test]
    fn translated_fixture_uses_its_current_origin() {
        // Exact full-sized frame recorded in the failed human-operated run.
        let r = fixture_selection(frame(280., 315.)).expect("valid fixture");
        assert_eq!(
            (r.origin.x, r.origin.y, r.size.width, r.size.height),
            (296., 331., 680., 540.)
        );
    }
    #[test]
    fn requested_initial_origin_still_has_the_same_inset() {
        let r = fixture_selection(frame(60., 100.)).expect("valid fixture");
        assert_eq!((r.origin.x, r.origin.y), (76., 116.));
    }
    #[test]
    fn thumbnail_geometry_is_not_a_capture_target() {
        assert!(
            fixture_selection(Rect {
                origin: Point { x: -253., y: 613. },
                size: Size {
                    width: 37.,
                    height: 100.
                }
            })
            .is_err()
        );
    }
    #[test]
    fn preview_tracks_the_measured_selection() {
        let r = fixture_selection(frame(280., 315.)).expect("valid fixture");
        let right = preview_frame(r, screen(), PreviewPlacement::Right).expect("fits");
        let left = preview_frame(r, screen(), PreviewPlacement::Left).expect("fits");
        let inside = preview_frame(r, screen(), PreviewPlacement::Inside).expect("fits");
        assert_eq!((right.origin.x, right.origin.y), (984., 331.));
        assert_eq!((left.origin.x, left.origin.y), (48., 331.));
        assert_eq!(
            (inside.origin.x, inside.origin.y, inside.size.height),
            (736., 411., 460.)
        );
    }
}
