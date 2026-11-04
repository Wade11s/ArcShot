//! Scroll Capture permission preflight, target identification and own-window exclusion.
//!
//! Every function here is a safe adapter over public AppKit/CoreGraphics and
//! ScreenCaptureKit interfaces. The pure decision of whether preparation may
//! continue lives in `arcshot_core::scroll`; this module only observes and
//! reports. It never starts a capture, never reads pixels and never logs a
//! window title.
use std::{ffi::c_char, ffi::c_void};

use arcshot_core::{
    ScrollPermission, ScrollPermissions, ScrollTarget, ScrollTargetError, ScrollTargetFrame,
    ScrollWindowError, Selection,
};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSRunningApplication, NSWorkspace};
use screencapturekit::shareable_content::{SCShareableContent, SCWindow};

use crate::{CaptureError, DisplaySnapshot, PermissionError, owned_visible_window_ids};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCGWindowLayer: *const c_void;
    static kCGWindowBounds: *const c_void;
    static kCGWindowNumber: *const c_void;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: *const c_void, kind: isize, value: *mut c_void) -> bool;
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        text: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFRelease(value: *const c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// CoreFoundation's UTF-8 encoding constant.
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
/// `kCGWindowListOptionOnScreenOnly` (1 << 0), an enum value not an exported symbol.
const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
/// `kCGWindowListExcludeDesktopElements` (1 << 4), an enum value.
const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
/// `CFNumberLongLongType`, used for window layer and window number values.
const K_CF_NUMBER_LONG_LONG_TYPE: isize = 11;
/// `CFNumberDoubleType`, used for window bounds values.
const K_CF_NUMBER_DOUBLE_TYPE: isize = 13;

/// Reads the real three-permission Scroll preflight without requesting anything.
#[must_use]
pub fn scroll_permissions() -> ScrollPermissions {
    ScrollPermissions {
        // SAFETY: These system functions take no arguments and expose no borrowed data.
        screen_recording: unsafe { CGPreflightScreenCaptureAccess() },
        input_monitoring: unsafe { CGPreflightListenEventAccess() },
        accessibility: unsafe { AXIsProcessTrusted() },
    }
}

/// Requests one missing Scroll permission from the system.
///
/// Screen Recording and Input Monitoring have a direct request entry point;
/// Accessibility has no programmatic prompt, so opening its settings pane is the
/// only honest request. The return value is the system's answer, never a guess.
#[must_use]
pub fn request_scroll_permission(permission: ScrollPermission) -> bool {
    match permission {
        ScrollPermission::ScreenRecording => {
            // SAFETY: No arguments and no borrowed data.
            unsafe { CGRequestScreenCaptureAccess() }
        }
        ScrollPermission::InputMonitoring => {
            // SAFETY: No arguments and no borrowed data.
            unsafe { CGRequestListenEventAccess() }
        }
        // There is no public Accessibility prompt; the settings pane is the request.
        ScrollPermission::Accessibility => false,
    }
}

/// Opens the matching System Settings pane for one Scroll permission.
///
/// # Errors
/// Returns [`PermissionError`] when the URL cannot be built or the system refuses it.
pub fn open_scroll_permission_settings(
    permission: ScrollPermission,
) -> Result<(), PermissionError> {
    let url = match permission {
        ScrollPermission::ScreenRecording => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        ScrollPermission::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        ScrollPermission::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
    };
    crate::permission::open_settings_url(url)
}

/// Reads the pointer in CoreGraphics global display coordinates.
///
/// Returns `None` off the main thread or when AppKit cannot supply a location.
#[must_use]
pub fn pointer_location_cg(display: DisplaySnapshot) -> Option<[f64; 2]> {
    let _ = MainThreadMarker::new()?;
    // SAFETY: `mouseLocation` is a class method with no arguments; AppKit returns
    // a point in screen coordinates whose origin is the main display's bottom-left.
    let point = NSEvent::mouseLocation();
    Some([point.x, display.height - point.y])
}

/// Reads the frontmost application's process identifier.
///
/// Returns `None` off the main thread or when the workspace reports no frontmost app.
#[must_use]
pub fn frontmost_pid() -> Option<u32> {
    let _ = MainThreadMarker::new()?;
    let workspace = NSWorkspace::sharedWorkspace();
    let frontmost: Option<objc2::rc::Retained<NSRunningApplication>> =
        workspace.frontmostApplication();
    let pid = frontmost?.processIdentifier();
    u32::try_from(pid).ok()
}

/// Identifies and locks the single target window for one display-local selection.
///
/// This runs on a worker: `SCShareableContent::get()` can block, so the caller
/// must poll it under the same absolute deadline as a capture request. The
/// pointer location and frontmost application are read on the main thread and
/// injected here, so no AppKit object crosses the worker boundary.
///
/// # Errors
/// Returns a classified failure for an unidentifiable target, an off-screen or
/// non-frontmost window, a pointer mismatch, or an uncertain native state.
pub fn probe_scroll_target(
    selection: Selection,
    display: DisplaySnapshot,
    pointer: [f64; 2],
    frontmost_pid: u32,
) -> Result<ScrollTarget, ScrollTargetError> {
    let content = SCShareableContent::get().map_err(|_| ScrollTargetError::NativeUncertain)?;
    // The selection is display-local; SCK frames are in global display coordinates.
    let global = ScrollTargetFrame {
        x: display.x + selection.x(),
        y: display.y + selection.y(),
        width: selection.width(),
        height: selection.height(),
    };
    let mut candidates: Vec<SCWindow> = Vec::new();
    for window in content.windows() {
        if window.window_layer() != 0 || !window.is_on_screen() {
            continue;
        }
        let owner = match window.owning_application() {
            Some(owner) => owner,
            None => continue,
        };
        // ArcShot's own windows are never a Scroll target.
        if u32::try_from(owner.process_id()).ok() == Some(std::process::id()) {
            continue;
        }
        let frame = window.frame();
        if frame.size.width <= 0.0 || frame.size.height <= 0.0 {
            continue;
        }
        if global.x >= frame.origin.x
            && global.y >= frame.origin.y
            && global.x + global.width <= frame.origin.x + frame.size.width
            && global.y + global.height <= frame.origin.y + frame.size.height
        {
            candidates.push(window);
        }
    }
    match candidates.len() {
        0 => Err(ScrollTargetError::NoTarget),
        1 => Ok(()),
        _ => Err(ScrollTargetError::AmbiguousTarget),
    }?;
    let window = candidates.remove(0);
    let owner = window
        .owning_application()
        .ok_or(ScrollTargetError::NoTarget)?;
    if u32::try_from(owner.process_id()).ok() != Some(frontmost_pid) {
        return Err(ScrollTargetError::NotFrontmost);
    }
    // Cross-check the pointer with public CoreGraphics hit-testing. The target is
    // frontmost, so a pointer over the selection must resolve to the same window.
    if !cg_window_at(pointer).is_some_and(|id| id == window.window_id()) {
        return Err(ScrollTargetError::PointerMismatch);
    }
    let frame = window.frame();
    Ok(ScrollTarget {
        window_id: window.window_id(),
        owner: {
            let bundle = owner.bundle_identifier();
            if bundle.is_empty() {
                owner.application_name()
            } else {
                bundle
            }
        },
        frame: ScrollTargetFrame {
            x: frame.origin.x - display.x,
            y: frame.origin.y - display.y,
            width: frame.size.width,
            height: frame.size.height,
        },
    })
}

/// Fixes ArcShot's own visible window set and maps every window to an `SCWindow`.
///
/// # Errors
/// Returns [`ScrollWindowError::MappingFailed`] when a still-visible owned window
/// cannot be mapped or is not owned by this process, and
/// [`ScrollWindowError::NativeUncertain`] when the native query cannot be confirmed.
pub fn scroll_exclusion_set() -> Result<Vec<u32>, ScrollWindowError> {
    let ids = owned_visible_window_ids().map_err(|error| match error {
        CaptureError::NotMainThread => ScrollWindowError::NotMainThread,
        CaptureError::Window => ScrollWindowError::MappingFailed,
        _ => ScrollWindowError::NativeUncertain,
    })?;
    let content = SCShareableContent::get().map_err(|_| ScrollWindowError::NativeUncertain)?;
    let windows = content.windows();
    for id in &ids {
        let window = windows
            .iter()
            .find(|window| window.window_id() == *id)
            .ok_or(ScrollWindowError::MappingFailed)?;
        let owner = window
            .owning_application()
            .ok_or(ScrollWindowError::MappingFailed)?;
        if u32::try_from(owner.process_id()).ok() != Some(std::process::id()) {
            return Err(ScrollWindowError::MappingFailed);
        }
    }
    Ok(ids)
}

/// Returns the identifier of the first public layer-0 on-screen window at a point.
///
/// The CoreGraphics window list is ordered front-to-back, so the first match is
/// the window the pointer actually hits. Returns `None` when no window matches.
fn cg_window_at(point: [f64; 2]) -> Option<u32> {
    // SAFETY: The option constants and the null relative window are the documented
    // public form; the returned array is released below.
    let array = unsafe {
        CGWindowListCopyWindowInfo(
            K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            0,
        )
    };
    if array.is_null() {
        return None;
    }
    let count = unsafe { CFArrayGetCount(array) };
    let mut found = None;
    for index in 0..count {
        let window = unsafe { CFArrayGetValueAtIndex(array, index) };
        if window.is_null() {
            continue;
        }
        let layer = unsafe { CFDictionaryGetValue(window, kCGWindowLayer) };
        if unsafe { cf_integer(layer) } != Some(0) {
            continue;
        }
        let bounds = unsafe { CFDictionaryGetValue(window, kCGWindowBounds) };
        let (Some(x), Some(y), Some(width), Some(height)) = (
            unsafe { cf_bound(bounds, "X") },
            unsafe { cf_bound(bounds, "Y") },
            unsafe { cf_bound(bounds, "Width") },
            unsafe { cf_bound(bounds, "Height") },
        ) else {
            continue;
        };
        if point[0] >= x && point[0] <= x + width && point[1] >= y && point[1] <= y + height {
            let number = unsafe { CFDictionaryGetValue(window, kCGWindowNumber) };
            if let Some(id) = unsafe { cf_integer(number) } {
                found = Some(u32::try_from(id).ok()?);
                break;
            }
        }
    }
    // SAFETY: The array is a +0 CoreFoundation object returned by a Copy function.
    unsafe { CFRelease(array) };
    found
}

/// Reads one integer-valued CoreFoundation number.
///
/// # Safety
/// `value` must be null or a valid `CFNumber`.
unsafe fn cf_integer(value: *const c_void) -> Option<i64> {
    if value.is_null() {
        return None;
    }
    let mut out: i64 = 0;
    // SAFETY: The number is a valid CFNumber and `out` is a correctly sized local.
    if unsafe { CFNumberGetValue(value, K_CF_NUMBER_LONG_LONG_TYPE, (&raw mut out).cast()) } {
        Some(out)
    } else {
        None
    }
}

/// Reads one double-valued key from a CoreGraphics bounds dictionary.
///
/// # Safety
/// `dictionary` must be null or a valid `CFDictionary`.
unsafe fn cf_bound(dictionary: *const c_void, key: &str) -> Option<f64> {
    if dictionary.is_null() {
        return None;
    }
    // SAFETY: A null allocator selects the default allocator and the key is a
    // valid UTF-8 string with the documented UTF-8 encoding.
    let key = unsafe {
        CFStringCreateWithCString(
            std::ptr::null_mut(),
            key.as_ptr().cast::<c_char>(),
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if key.is_null() {
        return None;
    }
    let value = unsafe { CFDictionaryGetValue(dictionary, key) };
    // SAFETY: The key is a +1 object created above and is no longer needed.
    unsafe { CFRelease(key) };
    if value.is_null() {
        return None;
    }
    let mut out: f64 = 0.0;
    // SAFETY: The value is a valid CFNumber and `out` is a correctly sized local.
    if unsafe { CFNumberGetValue(value, K_CF_NUMBER_DOUBLE_TYPE, (&raw mut out).cast()) } {
        Some(out)
    } else {
        None
    }
}
