//! Scroll Capture permission preflight, target identification and own-window exclusion.
//!
//! Every function here is a safe adapter over public AppKit/CoreGraphics and
//! ScreenCaptureKit interfaces. The pure decision of whether preparation may
//! continue lives in `arcshot_core::scroll`; this module only observes and
//! reports. It never starts a capture, never reads pixels and never logs a
//! window title.
use std::ffi::c_void;

use apple_cf::cg::CGRect;
use arcshot_core::{
    ScrollPermission, ScrollPermissions, ScrollTarget, ScrollTargetError, ScrollTargetFrame,
    ScrollWindowError, Selection,
};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplicationActivationOptions, NSEvent, NSRunningApplication, NSWorkspace};
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
    static kCGWindowOwnerPID: *const c_void;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: *const c_void, kind: isize, value: *mut c_void) -> bool;
    fn CFRelease(value: *const c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGRectMakeWithDictionaryRepresentation(dictionary: *const c_void, rect: *mut CGRect)
    -> bool;
}

/// `kCGWindowListOptionOnScreenOnly` (1 << 0), an enum value not an exported symbol.
const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
/// `kCGWindowListExcludeDesktopElements` (1 << 4), an enum value.
const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
/// `CFNumberLongLongType`, used for window layer and window number values.
const K_CF_NUMBER_LONG_LONG_TYPE: isize = 11;
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
/// pointer location is read on the main thread and injected here, so no AppKit
/// object crosses the worker boundary. The target does not need to be the
/// frontmost application yet — the user just used ArcShot's overlay, so
/// ArcShot is legitimately frontmost at entry; the foreground gate runs after
/// preparation, when ArcShot activates the locked target and confirms it.
///
/// Identification cross-checks the two public sources (I-12): the candidate
/// must be an on-screen layer-0 SCK window that fully contains the selection,
/// and the visually topmost such window — by the front-to-back CoreGraphics
/// window order — must be the window the pointer's public hit-test resolves
/// to. A selection no window fully contains spans windows (`NoTarget`); a
/// CoreGraphics/SCK disagreement is unidentifiable (`AmbiguousTarget`); a
/// pointer over a different window is a `PointerMismatch`.
///
/// # Errors
/// Returns a classified failure for an unidentifiable target, an off-screen
/// window, a pointer mismatch, or an uncertain native state.
pub fn probe_scroll_target(
    selection: Selection,
    display: DisplaySnapshot,
    pointer: [f64; 2],
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
    if candidates.is_empty() {
        return Err(ScrollTargetError::NoTarget);
    }
    // The CoreGraphics window list is ordered front-to-back; the visible
    // target is the topmost window that fully contains the selection.
    let order = cg_window_list_ids();
    let topmost = candidates
        .iter()
        .filter_map(|window| {
            order
                .iter()
                .position(|id| *id == window.window_id())
                .map(|position| (position, window))
        })
        .min_by_key(|(position, _)| *position)
        .map(|(_, window)| window);
    let Some(window) = topmost else {
        // SCK reports containing windows CoreGraphics does not see on screen;
        // the public sources disagree, so the target is not identifiable.
        return Err(ScrollTargetError::AmbiguousTarget);
    };
    let owner = window
        .owning_application()
        .ok_or(ScrollTargetError::NoTarget)?;
    let owner_pid = u32::try_from(owner.process_id()).map_err(|_| ScrollTargetError::NoTarget)?;
    // Cross-check the pointer with public CoreGraphics hit-testing. The
    // selection was just drawn over this window, so the pointer that drew it
    // must resolve to the same public window identity.
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
        owner_pid,
        frame: ScrollTargetFrame {
            x: frame.origin.x - display.x,
            y: frame.origin.y - display.y,
            width: frame.size.width,
            height: frame.size.height,
        },
    })
}

/// Brings the locked target application to the front and reports success.
///
/// This is the foreground half of the I-12 target gate: after ArcShot dismisses
/// its overlay, the target must own the foreground so raw scroll reaches it.
/// ArcShot is the foreground application at this moment, so cooperative
/// activation is permitted; the deprecated force-activation flag is not used.
///
/// # Errors
/// Returns [`ScrollTargetError::NoTarget`] when the process is gone and
/// [`ScrollTargetError::NotFrontmost`] when the activation was refused or the
/// target did not actually come to the front.
pub fn activate_scroll_target(owner_pid: u32) -> Result<(), ScrollTargetError> {
    let _ = MainThreadMarker::new().ok_or(ScrollTargetError::NotFrontmost)?;
    // SAFETY: The class method takes a plain pid and returns a retained object
    // or nil for a terminated process; nothing outlives the call.
    let running =
        { NSRunningApplication::runningApplicationWithProcessIdentifier(owner_pid as i32) };
    let running = running.ok_or(ScrollTargetError::NoTarget)?;
    // Cooperative activation with no options; the boolean result is whether
    // the system accepted the request.
    if !running.activateWithOptions(NSApplicationActivationOptions::empty()) {
        return Err(ScrollTargetError::NotFrontmost);
    }
    if frontmost_pid() != Some(owner_pid) {
        return Err(ScrollTargetError::NotFrontmost);
    }
    Ok(())
}

/// Re-checks the locked target with public interfaces during collection.
///
/// Runs on the main thread on a slow cadence: the window must still exist as an
/// on-screen window of the same process with the same geometry, and the target
/// application must still own the foreground. The locked frame is
/// display-local; the public frame is global, so the display origin converts.
///
/// # Errors
/// Returns the classified loss: `NoTarget`, `Geometry` or `NotFrontmost`.
pub fn scroll_target_intact(
    target: &ScrollTarget,
    display: DisplaySnapshot,
) -> Result<(), ScrollTargetError> {
    let Some(frontmost) = frontmost_pid() else {
        return Err(ScrollTargetError::NotFrontmost);
    };
    if frontmost != target.owner_pid {
        return Err(ScrollTargetError::NotFrontmost);
    }
    let Some((x, y, width, height, pid)) = cg_window_frame(target.window_id) else {
        return Err(ScrollTargetError::NoTarget);
    };
    if pid != target.owner_pid {
        return Err(ScrollTargetError::NoTarget);
    }
    let moved = (x - display.x - target.frame.x).abs() > 0.5
        || (y - display.y - target.frame.y).abs() > 0.5
        || (width - target.frame.width).abs() > 0.5
        || (height - target.frame.height).abs() > 0.5;
    if moved {
        return Err(ScrollTargetError::Geometry);
    }
    Ok(())
}

/// Reads one owned window's global on-screen frame by public window number.
///
/// Returns `(display-local x, y, width, height)` and the owning process id.
/// Windows that are not on screen report `None`.
fn cg_window_frame(window_id: u32) -> Option<(f64, f64, f64, f64, u32)> {
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
        let number = unsafe { CFDictionaryGetValue(window, kCGWindowNumber) };
        let Some(id) = (unsafe { cf_integer(number) }).and_then(|id| u32::try_from(id).ok()) else {
            continue;
        };
        if id != window_id {
            continue;
        }
        let Some((x, y, width, height)) = (unsafe { cf_window_bounds(window) }) else {
            break;
        };
        let owner = unsafe { CFDictionaryGetValue(window, kCGWindowOwnerPID) };
        let Some(pid) = (unsafe { cf_integer(owner) }).and_then(|pid| u32::try_from(pid).ok())
        else {
            break;
        };
        found = Some((x, y, width, height, pid));
        break;
    }
    // SAFETY: The array is a +0 CoreFoundation object returned by a Copy function.
    unsafe { CFRelease(array) };
    found
}

/// Reads the global on-screen frames of ArcShot's own visible windows.
///
/// Every entry mirrors one ID from [`crate::owned_visible_window_ids`]; the
/// frames anchor the tap's own-UI pass-through rect. A missing frame means a
/// window closed between the two enumerations and is a mapping failure.
///
/// # Errors
/// Returns an error when the public query fails or a window has no frame.
pub fn owned_visible_window_frames(
    ids: &[u32],
) -> Result<Vec<(u32, crate::scroll_input::GlobalRect)>, ScrollWindowError> {
    let mut frames = Vec::new();
    for id in ids {
        let Some((x, y, width, height, _pid)) = cg_window_frame(*id) else {
            return Err(ScrollWindowError::MappingFailed);
        };
        frames.push((
            *id,
            crate::scroll_input::GlobalRect {
                x,
                y,
                width,
                height,
            },
        ));
    }
    Ok(frames)
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

/// Returns the on-screen layer-0 window ids in front-to-back order.
///
/// The CoreGraphics window list is documented front-to-back, so the first id
/// covering a point is the window the user actually sees there.
fn cg_window_list_ids() -> Vec<u32> {
    // SAFETY: The option constants and the null relative window are the documented
    // public form; the returned array is released below.
    let array = unsafe {
        CGWindowListCopyWindowInfo(
            K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            0,
        )
    };
    if array.is_null() {
        return Vec::new();
    }
    let count = unsafe { CFArrayGetCount(array) };
    let mut ids = Vec::new();
    for index in 0..count {
        let window = unsafe { CFArrayGetValueAtIndex(array, index) };
        if window.is_null() {
            continue;
        }
        let layer = unsafe { CFDictionaryGetValue(window, kCGWindowLayer) };
        if unsafe { cf_integer(layer) } != Some(0) {
            continue;
        }
        let number = unsafe { CFDictionaryGetValue(window, kCGWindowNumber) };
        if let Some(id) = unsafe { cf_integer(number) }.and_then(|id| u32::try_from(id).ok()) {
            ids.push(id);
        }
    }
    // SAFETY: The array is a +0 CoreFoundation object returned by a Copy function.
    unsafe { CFRelease(array) };
    ids
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
        let Some((x, y, width, height)) = (unsafe { cf_window_bounds(window) }) else {
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

/// Reads one window entry's `kCGWindowBounds` as global display coordinates.
///
/// Uses `CGRectMakeWithDictionaryRepresentation`, the documented public
/// conversion for the bounds dictionaries CoreGraphics returns; no string keys
/// are reconstructed.
///
/// # Safety
/// `window` must be null or a valid window-info `CFDictionary`.
unsafe fn cf_window_bounds(window: *const c_void) -> Option<(f64, f64, f64, f64)> {
    if window.is_null() {
        return None;
    }
    // SAFETY: The value of the documented bounds key is a CFDictionary of the
    // form the conversion API consumes, and `rect` is a correctly sized local.
    let bounds = unsafe { CFDictionaryGetValue(window, kCGWindowBounds) };
    if bounds.is_null() {
        return None;
    }
    let mut rect = CGRect::new(0.0, 0.0, 0.0, 0.0);
    if !unsafe { CGRectMakeWithDictionaryRepresentation(bounds, &raw mut rect) } {
        return None;
    }
    Some((
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    ))
}
