//! Main-thread-only AppKit/Quartz boundary for this throwaway experiment.
use anyhow::{Result, bail, ensure};
use gpui::Window;
use objc::{class, msg_send, sel, sel_impl};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ffi::c_void,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};

type Ref = *mut c_void;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.origin.x
            && p.y >= self.origin.y
            && p.x < self.origin.x + self.size.width
            && p.y < self.origin.y + self.size.height
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: unsafe extern "C" fn(Ref, u32, Ref, Ref) -> Ref,
        info: Ref,
    ) -> Ref;
    fn CGEventTapEnable(tap: Ref, enabled: bool);
    fn CGEventTapIsEnabled(tap: Ref) -> bool;
    fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
    fn CGEventGetLocation(event: Ref) -> Point;
    fn CGEventGetFlags(event: Ref) -> u64;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGGetActiveDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayIsBuiltin(id: u32) -> u32;
    fn CGDisplayBounds(id: u32) -> Rect;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: Ref;
    fn CFRunLoopGetMain() -> Ref;
    fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
    fn CFRunLoopAddSource(runloop: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRemoveSource(runloop: Ref, source: Ref, mode: Ref);
    fn CFMachPortInvalidate(port: Ref);
    fn CFRelease(object: Ref);
}
#[derive(Debug, Default)]
pub struct Input {
    /// 0 running, 1 Finish, 2 cancel, 3 restart, 4 processing failure, 5 lost tap, 6 focus lost.
    pub command: AtomicU8,
    pub protection_lost: AtomicBool,
    pub selection_ready: AtomicBool,
    pub scrolls: AtomicU64,
    pub clicks: AtomicU64,
}

pub fn peak_rss_bytes() -> i64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the correctly sized struct on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } == 0 {
        // SAFETY: Successful getrusage initialized every field; macOS reports bytes.
        unsafe { usage.assume_init().ru_maxrss }
    } else {
        -1
    }
}
pub fn permissions() -> (bool, bool, bool) {
    // SAFETY: These synchronous preflights take no pointers or ownership.
    unsafe {
        (
            CGPreflightScreenCaptureAccess(),
            CGPreflightListenEventAccess(),
            AXIsProcessTrusted(),
        )
    }
}
pub fn request_permissions() {
    // SAFETY: Called only following a user press of the permissions button on main thread.
    unsafe {
        CGRequestScreenCaptureAccess();
        CGRequestListenEventAccess();
    }
    // Accessibility is deliberately a human Settings action; don't use private TCC APIs.
}
pub fn display() -> Result<(u32, Rect)> {
    let mut ids = [0; 16];
    let mut count = 0;
    // SAFETY: Buffer contains 16 IDs; count is a live out-pointer.
    let err = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) };
    ensure!(
        err == 0 && count == 1,
        "prototype requires exactly one active built-in display"
    );
    let id = ids[0];
    // SAFETY: Active display ID just returned by Quartz.
    ensure!(
        unsafe { CGDisplayIsBuiltin(id) } != 0,
        "active display is not built-in"
    );
    // SAFETY: Same active display ID.
    Ok((id, unsafe { CGDisplayBounds(id) }))
}

struct TapState {
    input: Arc<Input>,
    rect: Option<Rect>,
    held_mouse: u64,
    held_keys: u128,
}
pub struct Tap {
    port: Ref,
    source: Ref,
    state: Box<TapState>,
}

impl Tap {
    pub fn new(input: Arc<Input>) -> Result<Self> {
        let (screen, listen, ax) = permissions();
        ensure!(
            screen && listen && ax,
            "grant Screen Recording, Input Monitoring and Accessibility, then relaunch the prototype"
        );
        // SAFETY: No pointers; don't start halfway through a physical gesture.
        unsafe {
            ensure!(
                !(0..64).any(|b| CGEventSourceButtonState(1, b)),
                "release all mouse buttons before starting"
            );
            ensure!(
                ![36, 76, 53, 0].iter().any(|&k| CGEventSourceKeyState(1, k)),
                "release capture control keys before starting"
            );
        }
        let mut state = Box::new(TapState {
            input,
            rect: None,
            held_mouse: 0,
            held_keys: 0,
        });
        let events = [1, 2, 3, 4, 6, 7, 10, 11, 22, 25, 26, 27];
        let mask = events.iter().fold(0, |m, &e| m | (1u64 << e));
        // SAFETY: state has a stable boxed address until after the main-runloop tap is invalidated.
        let port = unsafe {
            CGEventTapCreate(
                1,
                0,
                0,
                mask,
                callback,
                (&mut *state as *mut TapState).cast(),
            )
        };
        ensure!(!port.is_null(), "active Quartz tap denied");
        // SAFETY: port is a valid +1 CFMachPort.
        let source = unsafe { CFMachPortCreateRunLoopSource(ptr::null_mut(), port, 0) };
        if source.is_null() {
            // SAFETY: Partial installation owns this port and no source has been installed.
            unsafe {
                CFMachPortInvalidate(port);
                CFRelease(port);
            }
            bail!("cannot create event runloop source");
        }
        let tap = Self {
            port,
            source,
            state,
        };
        // SAFETY: GPUI owns the main runloop; we add a source without running a nested loop.
        unsafe {
            CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(port, true);
            ensure!(CGEventTapIsEnabled(port), "tap is not enabled");
        }
        Ok(tap)
    }
    pub fn prepare_selection(&mut self, rect: Rect) {
        self.state.rect = Some(rect);
        self.state
            .input
            .selection_ready
            .store(true, Ordering::SeqCst);
    }
    pub fn drained(&mut self) -> bool {
        let command = self.state.input.command.load(Ordering::SeqCst);
        // A disabled tap cannot receive releases: explicitly lost protection, not a
        // successful drain. Otherwise keep release-only interception until paired ups.
        self.state.input.protection_lost.load(Ordering::SeqCst)
            || command == 5
            || (command != 0 && self.state.held_keys == 0 && self.state.held_mouse == 0)
    }
}
impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: Created/destroyed only on main thread between callbacks. State remains
        // alive through source removal and invalidation. Both Create-rule objects are +1.
        unsafe {
            CGEventTapEnable(self.port, false);
            CFRunLoopRemoveSource(CFRunLoopGetMain(), self.source, kCFRunLoopCommonModes);
            CFMachPortInvalidate(self.port);
            CFRelease(self.source);
            CFRelease(self.port);
        }
    }
}
unsafe extern "C" fn callback(_proxy: Ref, ty: u32, event: Ref, info: Ref) -> Ref {
    // SAFETY: Tap owns this stable state until main-runloop invalidation, between callbacks.
    let state = unsafe { &mut *info.cast::<TapState>() };
    let active = state.input.command.load(Ordering::SeqCst) == 0;
    // No allocation, blocking, AppKit calls, indexing, cleanup or panicking operations.
    match ty {
        0xffff_fffe | 0xffff_ffff => {
            state.input.protection_lost.store(true, Ordering::SeqCst);
            state.input.command.store(5, Ordering::SeqCst);
            event
        }
        22 => {
            if !active {
                return event;
            } // release-only draining does not seize new input
            // SAFETY: Quartz borrows a live scroll event for this callback.
            let point = unsafe { CGEventGetLocation(event) };
            if state.rect.is_some_and(|rect| rect.contains(point)) {
                state.input.scrolls.fetch_add(1, Ordering::Relaxed);
                event // preserve original phase/momentum, never repost
            } else {
                ptr::null_mut()
            }
        }
        1 | 2 | 3 | 4 | 6 | 7 | 25 | 26 | 27 => {
            // SAFETY: Field 3 is the physical mouse button number.
            let button = unsafe { CGEventGetIntegerValueField(event, 3) };
            if !(0..64).contains(&button) {
                state.input.protection_lost.store(true, Ordering::SeqCst);
                state.input.command.store(5, Ordering::SeqCst);
                return event;
            }
            let bit = 1u64 << button;
            let owned = state.held_mouse & bit != 0;
            if matches!(ty, 2 | 4 | 26) {
                state.held_mouse &= !bit;
                return if owned { ptr::null_mut() } else { event };
            }
            if !active && !owned {
                return event;
            }
            if matches!(ty, 1 | 3 | 25) {
                state.held_mouse |= bit;
                state.input.clicks.fetch_add(1, Ordering::Relaxed);
                // SAFETY: A mouse down supplies an integer click-state field.
                if ty == 1 && unsafe { CGEventGetIntegerValueField(event, 1) } >= 2 {
                    let _ = state.input.command.compare_exchange(
                        0,
                        1,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                }
            }
            ptr::null_mut() // FIRST down and every owned drag/up, even after Finish
        }
        10 | 11 => {
            // SAFETY: Borrowed Quartz keyboard event, SDK integer fields and flags.
            let key = unsafe { CGEventGetIntegerValueField(event, 9) };
            if !(0..128).contains(&key) {
                return event;
            }
            let bit = 1u128 << key;
            let owned = state.held_keys & bit != 0;
            if ty == 11 {
                state.held_keys &= !bit;
                return if owned { ptr::null_mut() } else { event };
            }
            if owned {
                return ptr::null_mut();
            }
            // Never steal a repeat whose original down was passed through.
            if !active || unsafe { CGEventGetIntegerValueField(event, 8) } != 0 {
                return event;
            }
            // SAFETY: Same borrowed keyboard event.
            let flags = unsafe { CGEventGetFlags(event) };
            let command = match key {
                36 | 76 => 1,
                53 => 2,
                0 if flags & (1 << 20) != 0 && flags & (1 << 17) != 0 => 3,
                _ => return event,
            };
            state.held_keys |= bit;
            let _ = state.input.command.compare_exchange(
                0,
                command,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            ptr::null_mut()
        }
        _ => event,
    }
}

pub fn window_number(window: &Window) -> Result<u32> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|e| anyhow::anyhow!("native handle unavailable: {e}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        bail!("not an AppKit window");
    };
    // SAFETY: Borrowed live GPUI NSView and its owning window; main-thread only.
    unsafe {
        let view = handle.ns_view.as_ptr().cast::<objc::runtime::Object>();
        let ns_window: *mut objc::runtime::Object = msg_send![view, window];
        ensure!(!ns_window.is_null(), "view has no NSWindow");
        let number: isize = msg_send![ns_window, windowNumber];
        Ok(number.try_into()?)
    }
}
pub fn preview_window(window: &Window, passthrough: bool) -> Result<()> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|e| anyhow::anyhow!("native handle unavailable: {e}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        bail!("not an AppKit window");
    };
    // SAFETY: GPUI's borrowed handle is an NSView. Accessed on main thread while alive.
    unsafe {
        let view = handle.ns_view.as_ptr().cast::<objc::runtime::Object>();
        let ns_window: *mut objc::runtime::Object = msg_send![view, window];
        ensure!(!ns_window.is_null(), "preview has no NSWindow");
        let _: () = msg_send![ns_window, setIgnoresMouseEvents: if passthrough { objc::runtime::YES } else { objc::runtime::NO }];
        let _: () = msg_send![ns_window, setLevel: 3isize];
    }
    Ok(())
}
pub fn move_preview(window: &mut Window, frame: Rect) -> Result<()> {
    let (_, screen) = display()?;
    window.resize(gpui::size(
        gpui::px(frame.size.width as f32),
        gpui::px(frame.size.height as f32),
    ));
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|e| anyhow::anyhow!("native handle unavailable: {e}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        bail!("not an AppKit window");
    };
    // SAFETY: Borrowed live GPUI NSView and NSWindow, on main thread. NSPoint is the
    // AppKit binding's correctly encoded argument; the single active built-in screen
    // has Quartz top-left vs AppKit bottom-left origins. This does not activate a window.
    unsafe {
        let view = handle.ns_view.as_ptr().cast::<objc::runtime::Object>();
        let ns_window: *mut objc::runtime::Object = msg_send![view, window];
        ensure!(!ns_window.is_null(), "preview has no NSWindow");
        let top_left = cocoa::foundation::NSPoint::new(
            frame.origin.x,
            screen.origin.y + screen.size.height - frame.origin.y,
        );
        let _: () = msg_send![ns_window, setFrameTopLeftPoint:top_left];
    }
    Ok(())
}
pub fn target_is_foreground(pid: u32) -> bool {
    // SAFETY: Main-thread AppKit query; no autoreleased pointer escapes.
    unsafe {
        let workspace: *mut objc::runtime::Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front: *mut objc::runtime::Object = msg_send![workspace, frontmostApplication];
        if front.is_null() {
            return false;
        }
        let front_pid: i32 = msg_send![front, processIdentifier];
        front_pid == pid as i32
    }
}
pub fn activate_target(pid: u32) -> Result<()> {
    // SAFETY: Main-thread AppKit calls with a process ID; no retained pointer escapes.
    unsafe {
        let target: *mut objc::runtime::Object = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier:pid as i32];
        ensure!(!target.is_null(), "target process unavailable");
        let accepted: objc::runtime::BOOL = msg_send![target, activateWithOptions:2usize];
        ensure!(accepted == objc::runtime::YES, "target activation rejected");
    }
    ensure!(
        target_is_foreground(pid),
        "target not foreground yet; retry after activation"
    );
    Ok(())
}
pub fn copy_png(bytes: &[u8]) -> Result<()> {
    // SAFETY: Main-thread AppKit call. NSData copies the borrowed bytes before returning;
    // all autoreleased objects remain alive during this synchronous call.
    unsafe {
        let pb: *mut objc::runtime::Object = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: isize = msg_send![pb, clearContents];
        let data: *mut objc::runtime::Object =
            msg_send![class!(NSData), dataWithBytes:bytes.as_ptr() length:bytes.len()];
        let kind: *mut objc::runtime::Object =
            msg_send![class!(NSString), stringWithUTF8String:c"public.png".as_ptr()];
        let ok: objc::runtime::BOOL = msg_send![pb, setData:data forType:kind];
        ensure!(ok == objc::runtime::YES, "PNG clipboard write rejected");
    }
    Ok(())
}
