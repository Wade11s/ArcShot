//! Single-display ScreenCaptureKit adapter. No operation here starts a worker.

use std::{
    error::Error,
    ffi::c_void,
    fmt,
    time::{Duration, Instant},
};

use apple_cf::cg::{CGRect, Display};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use screencapturekit::{
    screenshot_manager::SCScreenshotManager,
    shareable_content::SCShareableContent,
    stream::{
        configuration::{PixelFormat, SCCaptureDynamicRange, SCStreamConfiguration, color_space},
        content_filter::SCContentFilter,
    },
};

const MAX_SIDE: usize = 16_384;
const MAX_PIXELS: usize = 24_000_000;
// Bound both the native source storage and our additional destination surface.
const MAX_SOURCE_BYTES: usize = 192_000_000;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayIsBuiltin(id: u32) -> u32;
    fn CGDisplayIsOnline(id: u32) -> u32;
    static kCGColorSpaceSRGB: *const c_void;
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits: usize,
        stride: usize,
        space: *mut c_void,
        info: u32,
    ) -> *mut c_void;
    fn CGContextDrawImage(context: *mut c_void, rect: CGRect, image: *const c_void);
    fn CGContextRelease(context: *mut c_void);
}

/// A fixed built-in display in global logical points and its physical pixel scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplaySnapshot {
    /// CoreGraphics display identifier.
    pub id: u32,
    /// Global logical-point origin.
    pub x: f64,
    /// Global logical-point origin.
    pub y: f64,
    /// Logical-point width.
    pub width: f64,
    /// Logical-point height.
    pub height: f64,
    /// Physical pixels per logical point.
    pub scale: f64,
}

/// One tightly packed, opaque, named-sRGB BGRA8 display image.
#[derive(Debug)]
pub struct NativeCapture {
    /// Display whose geometry was captured.
    pub display: DisplaySnapshot,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Bytes per row.
    pub stride: usize,
    /// Top-to-bottom BGRA8 pixels.
    pub bgra: Vec<u8>,
    /// Time spent in the synchronous SCScreenshotManager.capture_image call only.
    pub request_duration: Duration,
}

/// Native failures, separated by whether request state is uncertain.
#[derive(Debug)]
#[non_exhaustive]
pub enum CaptureError {
    /// A ScreenCaptureKit request failed or timed out; its native state is uncertain.
    Request(String),
    /// The required built-in single-display topology or geometry is unavailable.
    Display,
    /// A visible owned window could not be mapped to a native window ID.
    Window,
    /// The frame or its conversion is invalid.
    Frame,
    /// The frame or allocation would exceed the adapter's resource limits.
    Budget,
    /// An AppKit-only operation was called away from the main thread.
    NotMainThread,
}

impl CaptureError {
    /// Whether the process must not retry capture before relaunch.
    #[must_use]
    pub fn requires_relaunch(&self) -> bool {
        matches!(self, Self::Request(_))
    }
}
impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(message) => write!(f, "uncertain screen capture request: {message}"),
            Self::Display => f.write_str("built-in display unavailable or changed"),
            Self::Window => f.write_str("visible owned window cannot be excluded"),
            Self::Frame => f.write_str("invalid captured frame"),
            Self::Budget => f.write_str("capture exceeds resource budget"),
            Self::NotMainThread => f.write_str("window enumeration requires the main thread"),
        }
    }
}
impl Error for CaptureError {}

/// Returns the sole active, online built-in 2x display, without an external fallback.
///
/// # Errors
/// Returns an error if topology, mode, or geometry cannot be established.
pub fn builtin_display() -> Result<DisplaySnapshot, CaptureError> {
    let displays = Display::active();
    if displays.len() != 1 {
        return Err(CaptureError::Display);
    }
    let display = displays[0];
    // SAFETY: CoreGraphics accepts any display ID and returns a Boolean integer.
    if unsafe { CGDisplayIsBuiltin(display.id()) == 0 || CGDisplayIsOnline(display.id()) == 0 } {
        return Err(CaptureError::Display);
    }
    let bounds = display.bounds();
    let mode = display.mode().ok_or(CaptureError::Display)?;
    if mode.width() == 0 || mode.height() == 0 {
        return Err(CaptureError::Display);
    }
    let scale_x = mode.pixel_width() as f64 / mode.width() as f64;
    let scale_y = mode.pixel_height() as f64 / mode.height() as f64;
    let snapshot = DisplaySnapshot {
        id: display.id(),
        x: bounds.origin.x,
        y: bounds.origin.y,
        width: bounds.size.width,
        height: bounds.size.height,
        scale: scale_x,
    };
    if !snapshot.x.is_finite()
        || !snapshot.y.is_finite()
        || !snapshot.width.is_finite()
        || !snapshot.height.is_finite()
        || !scale_x.is_finite()
        || snapshot.width <= 0.0
        || snapshot.height <= 0.0
        || scale_x != 2.0
        || scale_y != 2.0
    {
        return Err(CaptureError::Display);
    }
    Ok(snapshot)
}

/// Enumerates all currently visible AppKit windows owned by this process.
///
/// Call on the main thread immediately before scheduling capture. Do not create or
/// show another window until capture is complete. Each ID is mapped to an SCWindow
/// inside `capture_builtin`; any missing mapping fails closed.
///
/// # Errors
/// Returns an error off the main thread or for an invalid or duplicated window number.
pub fn owned_visible_window_ids() -> Result<Vec<u32>, CaptureError> {
    let marker = MainThreadMarker::new().ok_or(CaptureError::NotMainThread)?;
    let windows = NSApplication::sharedApplication(marker).windows();
    let mut ids = Vec::new();
    for window in windows.iter() {
        if window.isVisible() {
            let id = u32::try_from(window.windowNumber()).map_err(|_| CaptureError::Window)?;
            if id == 0 || ids.contains(&id) {
                return Err(CaptureError::Window);
            }
            ids.push(id);
        }
    }
    Ok(ids)
}

fn dimensions(snapshot: DisplaySnapshot) -> Result<(u32, u32, usize, usize), CaptureError> {
    let w = snapshot.width * snapshot.scale;
    let h = snapshot.height * snapshot.scale;
    if !w.is_finite()
        || !h.is_finite()
        || w < 1.0
        || h < 1.0
        || w.fract() != 0.0
        || h.fract() != 0.0
        || w > MAX_SIDE as f64
        || h > MAX_SIDE as f64
    {
        return Err(CaptureError::Budget);
    }
    let width = w as usize;
    let height = h as usize;
    let pixels = width.checked_mul(height).ok_or(CaptureError::Budget)?;
    if pixels > MAX_PIXELS {
        return Err(CaptureError::Budget);
    }
    let stride = width.checked_mul(4).ok_or(CaptureError::Budget)?;
    let len = stride.checked_mul(height).ok_or(CaptureError::Budget)?;
    Ok((width as u32, height as u32, stride, len))
}

/// Captures the expected built-in display synchronously, excluding exact window IDs.
///
/// Call on a worker, never on the UI thread. The caller must implement an absolute
/// five-second watchdog starting before this call, catch worker panic, and prohibit
/// restart while a worker remains alive or after any uncertain request outcome.
/// An SCK internal timeout does not establish that the native request stopped.
///
/// # Errors
/// Returns a classified failure for display changes, exclusion, budget, frame, or
/// uncertain ScreenCaptureKit requests.
pub fn capture_builtin(
    expected: DisplaySnapshot,
    excluding_window_ids: Vec<u32>,
) -> Result<NativeCapture, CaptureError> {
    if builtin_display()? != expected {
        return Err(CaptureError::Display);
    }
    let (width, height, stride, len) = dimensions(expected)?;
    let content = SCShareableContent::get().map_err(|e| CaptureError::Request(e.to_string()))?;
    if builtin_display()? != expected {
        return Err(CaptureError::Display);
    }
    let displays = content.displays();
    let display = displays
        .iter()
        .find(|d| d.display_id() == expected.id)
        .ok_or(CaptureError::Display)?;
    let frame = display.frame();
    if frame.origin.x != expected.x
        || frame.origin.y != expected.y
        || frame.size.width != expected.width
        || frame.size.height != expected.height
    {
        return Err(CaptureError::Display);
    }
    let windows = content.windows();
    if excluding_window_ids.len() > windows.len() {
        return Err(CaptureError::Window);
    }
    let mut selected = Vec::new();
    selected
        .try_reserve_exact(excluding_window_ids.len())
        .map_err(|_| CaptureError::Budget)?;
    for id in excluding_window_ids {
        if id == 0
            || selected
                .iter()
                .any(|window: &&screencapturekit::shareable_content::SCWindow| {
                    window.window_id() == id
                })
        {
            return Err(CaptureError::Window);
        }
        let window = windows
            .iter()
            .find(|w| w.window_id() == id)
            .ok_or(CaptureError::Window)?;
        let owner = window.owning_application().ok_or(CaptureError::Window)?;
        if u32::try_from(owner.process_id()).ok() != Some(std::process::id()) {
            return Err(CaptureError::Window);
        }
        selected.push(window);
    }
    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&selected)
        .build()
        .map_err(|e| CaptureError::Request(e.to_string()))?;
    if f64::from(filter.point_pixel_scale()) != expected.scale {
        return Err(CaptureError::Display);
    }
    let config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_pixel_format(PixelFormat::BGRA)
        .with_shows_cursor(false)
        .with_source_rect(CGRect::new(0.0, 0.0, expected.width, expected.height))
        .with_color_space_name(color_space::SRGB)
        .map_err(|_| CaptureError::Frame)?
        .with_capture_dynamic_range(SCCaptureDynamicRange::SDR)
        .map_err(|_| CaptureError::Frame)?;
    let request_start = Instant::now();
    let image = SCScreenshotManager::capture_image(&filter, &config)
        .map_err(|e| CaptureError::Request(e.to_string()))?;
    let request_duration = request_start.elapsed();
    if builtin_display()? != expected {
        return Err(CaptureError::Display);
    }
    let source_stride = image.bytes_per_row();
    let min_source_stride = image
        .width()
        .checked_mul(image.bits_per_pixel())
        .and_then(|v| v.checked_add(7))
        .map(|v| v / 8)
        .ok_or(CaptureError::Budget)?;
    let source_bytes = source_stride
        .checked_mul(image.height())
        .ok_or(CaptureError::Budget)?;
    if image.width() != width as usize
        || image.height() != height as usize
        || image.bits_per_pixel() == 0
        || source_stride < min_source_stride
    {
        return Err(CaptureError::Frame);
    }
    if source_bytes > MAX_SOURCE_BYTES {
        return Err(CaptureError::Budget);
    }
    let mut bgra = Vec::new();
    bgra.try_reserve_exact(len)
        .map_err(|_| CaptureError::Budget)?;
    bgra.resize(len, 0);
    // SAFETY: The named system constant is valid for the duration of the call;
    // the context borrows our stable, initialized allocation for exactly its lifetime.
    // kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little yields BGRA8.
    unsafe {
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        if space.is_null() {
            return Err(CaptureError::Frame);
        }
        let context = CGBitmapContextCreate(
            bgra.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            stride,
            space,
            2 | (2 << 12),
        );
        CGColorSpaceRelease(space);
        if context.is_null() {
            return Err(CaptureError::Frame);
        }
        CGContextDrawImage(
            context,
            CGRect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            image.as_ptr(),
        );
        CGContextRelease(context);
    }
    if bgra.len() != len || bgra.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err(CaptureError::Frame);
    }
    if builtin_display()? != expected {
        return Err(CaptureError::Display);
    }
    Ok(NativeCapture {
        display: expected,
        width,
        height,
        stride,
        bgra,
        request_duration,
    })
}
