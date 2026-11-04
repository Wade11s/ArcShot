//! Quartz active-session event tap for Scroll collection (I-12 input contract).
//!
//! The tap is the input protection installed after native preparation: raw
//! scroll events inside the fixed selection reach the target, clicks (including
//! the first click of a double-click) never do, Enter/Escape belong to ArcShot,
//! and every swallowed press keeps ownership of its physical release. The
//! decision table lives in [`TapGate`], a pure, headless-testable state machine;
//! the C callback only translates one CGEvent into a [`TapEvent`] and applies
//! the decision. No overlay ever receives-and-forwards input: there is no
//! fallback mode.
//!
//! All methods run on the main thread only; the tap source is added to the main
//! run loop's common modes, so callback and coordinator never race.

use std::{cell::RefCell, error::Error, fmt, ptr, rc::Rc};

use objc2::MainThreadMarker;

type Ref = *mut core::ffi::c_void;

// CGEventType values from the macOS 27 SDK (EventTypes.h).
/// `kCGEventLeftMouseDown`.
const EVENT_LEFT_DOWN: u32 = 1;
/// `kCGEventLeftMouseUp`.
const EVENT_LEFT_UP: u32 = 2;
/// `kCGEventRightMouseDown`.
const EVENT_RIGHT_DOWN: u32 = 3;
/// `kCGEventRightMouseUp`.
const EVENT_RIGHT_UP: u32 = 4;
/// `kCGEventLeftMouseDragged`.
const EVENT_LEFT_DRAGGED: u32 = 6;
/// `kCGEventRightMouseDragged`.
const EVENT_RIGHT_DRAGGED: u32 = 7;
/// `kCGEventKeyDown`.
const EVENT_KEY_DOWN: u32 = 10;
/// `kCGEventKeyUp`.
const EVENT_KEY_UP: u32 = 11;
/// `kCGEventScrollWheel`.
const EVENT_SCROLL_WHEEL: u32 = 22;
/// `kCGEventOtherMouseDown`.
const EVENT_OTHER_DOWN: u32 = 25;
/// `kCGEventOtherMouseUp`.
const EVENT_OTHER_UP: u32 = 26;
/// `kCGEventOtherMouseDragged`.
const EVENT_OTHER_DRAGGED: u32 = 27;
/// `kCGEventTapDisabledByTimeout`.
const EVENT_TAP_DISABLED_TIMEOUT: u32 = 0xFFFF_FFFE;
/// `kCGEventTapDisabledByUserInput`.
const EVENT_TAP_DISABLED_INPUT: u32 = 0xFFFF_FFFF;

// Event field numbers (CGEventFields.h).
/// `kCGMouseEventClickState`.
const FIELD_CLICK_STATE: u32 = 1;
/// `kCGMouseEventButtonNumber`.
const FIELD_BUTTON_NUMBER: u32 = 3;
/// `kCGKeyboardEventAutorepeat`.
const FIELD_AUTOREPEAT: u32 = 8;
/// `kCGKeyboardEventKeycode`.
const FIELD_KEYCODE: u32 = 9;

/// `kCGEventTapDisabledBy*` is delivered as a null event pointer.
const NULL_EVENT: Ref = ptr::null_mut();

/// `CGPoint`: the return type of `CGEventGetLocation`.
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
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
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventGetLocation(event: Ref) -> CGPoint;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: Ref;
    fn CFRunLoopGetMain() -> Ref;
    fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
    fn CFRunLoopAddSource(run_loop: Ref, source: Ref, mode: Ref);
    fn CFRunLoopRemoveSource(run_loop: Ref, source: Ref, mode: Ref);
    fn CFMachPortInvalidate(port: Ref);
    fn CFRelease(object: *const core::ffi::c_void);
}

/// `kCGSessionEventTap`, `kCGHeadInsertEventTap`, `kCGEventTapOptionDefault`.
const SESSION_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const ACTIVE_FILTER: u32 = 0;
/// `kCGEventSourceStateHIDSystemState`: the physical hardware state, used for
/// the gesture preflight so a still-held button cannot start mid-gesture.
const HID_SYSTEM_STATE: i32 = 1;
/// The ANSI Return and keypad Enter keycodes ArcShot owns during collection.
const KEY_RETURN: u16 = 36;
const KEY_KEYPAD_ENTER: u16 = 76;
/// The ANSI Escape keycode.
const KEY_ESCAPE: u16 = 53;
/// Physical mouse buttons tracked by ownership bits.
const MOUSE_BUTTONS: u32 = 64;
/// Physical keycodes tracked by ownership bits.
const KEY_CODES: u16 = 128;

/// A rectangle in global display (top-left origin) logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlobalRect {
    /// Global left edge.
    pub x: f64,
    /// Global top edge.
    pub y: f64,
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
}

impl GlobalRect {
    /// Whether a global point lies inside the rectangle (half-open).
    #[must_use]
    pub fn contains(self, point: [f64; 2]) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && point[0] >= self.x
            && point[0] < self.x + self.width
            && point[1] >= self.y
            && point[1] < self.y + self.height
    }
}

/// One translated CGEvent, decoupled from CoreGraphics for the pure gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TapEvent {
    /// Which HID event this is.
    pub kind: TapEventKind,
    /// Physical mouse button number (mouse events only).
    pub button: u8,
    /// System click count (mouse down events only).
    pub click_state: i64,
    /// Physical keycode (keyboard events only).
    pub keycode: u16,
    /// Whether this key event is an autorepeat.
    pub autorepeat: bool,
    /// Event location in global display points (mouse/scroll events only).
    pub location: [f64; 2],
}

/// The HID event kinds the tap listens to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TapEventKind {
    /// Left button press.
    LeftDown,
    /// Left button release.
    LeftUp,
    /// Right button press.
    RightDown,
    /// Right button release.
    RightUp,
    /// Extra-button press (button ≥ 2).
    OtherDown,
    /// Extra-button release.
    OtherUp,
    /// Any drag (a held-button move).
    Dragged,
    /// Scroll wheel or trackpad scroll gesture.
    ScrollWheel,
    /// Key press.
    KeyDown,
    /// Key release.
    KeyUp,
    /// The system disabled the tap (timeout or user input).
    TapDisabled,
}

/// What the gate does with one event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TapDecision {
    /// Deliver the event unchanged; never repost or synthesize.
    Pass,
    /// Delete the event; ArcShot owns this gesture.
    Swallow,
}

/// A user intent the gate observed behind the protection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TapGateCommand {
    /// Enter or an ArcShot-owned double-click asked to complete.
    Finish,
    /// Escape asked to cancel.
    Cancel,
    /// The system disabled the tap; protection is explicitly lost.
    ProtectionLost,
}

/// Whether the tap is protecting, collecting or releasing owned input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TapMode {
    /// Installed; all events are held until the trusted first frame exists.
    Armed,
    /// Collection is open; raw scroll inside the selection reaches the target.
    Collecting,
    /// Collection ended; only releases of input we own are still swallowed.
    Draining,
}

/// Counters reported as text diagnostics when the tap is retired.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TapStats {
    /// Scroll events passed through to the target.
    pub scrolls_passed: u64,
    /// Click presses blocked from reaching the target.
    pub clicks_blocked: u64,
    /// Enter/Escape presses ArcShot owned.
    pub keys_owned: u64,
}

/// The pure decision core of the session tap.
///
/// Callback contract: never allocate, block, call AppKit or panic. The gate
/// keeps at most one queued command (first intent wins) exactly like the
/// validated prototype route.
#[derive(Debug)]
pub struct TapGate {
    mode: TapMode,
    selection: Option<GlobalRect>,
    control: Option<GlobalRect>,
    held_mouse: u64,
    held_keys: u128,
    command: Option<TapGateCommand>,
    /// Keycode whose press queued the command; its release is treated as
    /// already consumed at drain time because synthetic deliveries do not
    /// always produce a matching key-up at the HID session layer.
    command_key: Option<u16>,
    protection_lost: bool,
    stats: TapStats,
}

impl TapGate {
    /// A newly armed gate; everything is held until collection opens.
    #[must_use]
    pub fn armed() -> Self {
        Self {
            mode: TapMode::Armed,
            selection: None,
            control: None,
            held_mouse: 0,
            held_keys: 0,
            command: None,
            command_key: None,
            protection_lost: false,
            stats: TapStats::default(),
        }
    }

    /// Opens collection: raw scroll inside the selection now passes through.
    pub fn open_collection(&mut self, selection: GlobalRect) {
        self.selection = Some(selection);
        if self.mode == TapMode::Armed {
            self.mode = TapMode::Collecting;
        }
    }

    /// Re-opens collection after a failed completion attempt.
    pub fn reopen_collection(&mut self) {
        if self.mode == TapMode::Draining {
            self.mode = TapMode::Collecting;
        }
    }

    /// Sets the control window's global rect; events there reach ArcShot's UI.
    pub fn set_control_rect(&mut self, control: GlobalRect) {
        self.control = Some(control);
    }

    /// Ends collection; only releases of owned input stay swallowed.
    pub fn begin_drain(&mut self) {
        self.mode = TapMode::Draining;
        // The press that produced the command has been consumed by ArcShot; a
        // synthetic delivery may never send its key-up, so release that
        // ownership now instead of holding the native tap installed forever.
        if let Some(keycode) = self.command_key.take() {
            self.held_keys &= !(1u128 << keycode);
        }
    }

    /// Whether every owned press has seen its release.
    #[must_use]
    pub fn drained(&self) -> bool {
        self.mode == TapMode::Draining && self.held_mouse == 0 && self.held_keys == 0
    }

    /// Whether the system disabled the tap.
    ///
    /// Sticky: protection stays lost even after the command was consumed.
    #[must_use]
    pub const fn protection_lost(&self) -> bool {
        self.protection_lost
    }

    /// Takes the one queued user intent, if any.
    #[must_use]
    pub fn take_command(&mut self) -> Option<TapGateCommand> {
        self.command.take()
    }

    /// The counters reported when the tap is retired.
    #[must_use]
    pub const fn stats(&self) -> TapStats {
        self.stats
    }

    /// Applies one event and decides pass or swallow (I-12 ownership rules).
    #[must_use]
    pub fn on_event(&mut self, event: TapEvent) -> TapDecision {
        match event.kind {
            TapEventKind::TapDisabled => {
                // A disabled tap cannot receive releases: this is explicitly
                // lost protection, never a successful drain.
                self.protection_lost = true;
                if self.command.is_none() {
                    self.command = Some(TapGateCommand::ProtectionLost);
                }
                TapDecision::Pass
            }
            TapEventKind::ScrollWheel => {
                if self.mode == TapMode::Draining {
                    return TapDecision::Pass;
                }
                if self
                    .control
                    .is_some_and(|rect| rect.contains(event.location))
                {
                    return TapDecision::Pass;
                }
                if self.mode == TapMode::Collecting
                    && self
                        .selection
                        .is_some_and(|rect| rect.contains(event.location))
                {
                    self.stats.scrolls_passed += 1;
                    TapDecision::Pass
                } else {
                    // Before the trusted first frame, and outside the fixed
                    // selection, scrolls are held like every other gesture.
                    TapDecision::Swallow
                }
            }
            TapEventKind::KeyDown | TapEventKind::KeyUp => self.on_key(event),
            TapEventKind::LeftDown
            | TapEventKind::RightDown
            | TapEventKind::OtherDown
            | TapEventKind::LeftUp
            | TapEventKind::RightUp
            | TapEventKind::OtherUp
            | TapEventKind::Dragged => self.on_mouse(event),
        }
    }

    fn on_mouse(&mut self, event: TapEvent) -> TapDecision {
        // Events aimed at ArcShot's own control window are never intercepted:
        // the cancel button must stay operable during collection.
        if self
            .control
            .is_some_and(|rect| rect.contains(event.location))
        {
            return TapDecision::Pass;
        }
        let button = u64::from(event.button);
        if button >= MOUSE_BUTTONS as u64 {
            // An untracked button cannot be owned safely; treat as lost.
            if self.command.is_none() {
                self.command = Some(TapGateCommand::ProtectionLost);
            }
            return TapDecision::Pass;
        }
        let bit = 1 << button;
        let owned = self.held_mouse & bit != 0;
        let is_up = matches!(
            event.kind,
            TapEventKind::LeftUp | TapEventKind::RightUp | TapEventKind::OtherUp
        );
        if is_up {
            self.held_mouse &= !bit;
            // The release of a press we swallowed never leaks to the target.
            return if owned {
                TapDecision::Swallow
            } else {
                TapDecision::Pass
            };
        }
        if self.mode == TapMode::Draining && !owned {
            // New gestures after collection ended are the user's own.
            return TapDecision::Pass;
        }
        let is_down = matches!(
            event.kind,
            TapEventKind::LeftDown | TapEventKind::RightDown | TapEventKind::OtherDown
        );
        if is_down {
            self.held_mouse |= bit;
            self.stats.clicks_blocked += 1;
            // The second click of a double-click completes collection; the
            // first click was already swallowed, so nothing leaked.
            if self.mode == TapMode::Collecting
                && event.kind == TapEventKind::LeftDown
                && event.click_state >= 2
                && self.command.is_none()
            {
                self.command = Some(TapGateCommand::Finish);
            }
        }
        TapDecision::Swallow
    }

    fn on_key(&mut self, event: TapEvent) -> TapDecision {
        if event.keycode >= KEY_CODES {
            return TapDecision::Pass;
        }
        let bit = 1u128 << event.keycode;
        let owned = self.held_keys & bit != 0;
        if event.kind == TapEventKind::KeyUp {
            self.held_keys &= !bit;
            return if owned {
                TapDecision::Swallow
            } else {
                TapDecision::Pass
            };
        }
        // A repeat of a key we own stays owned; a repeat of a passed-through
        // key is never seized either.
        if owned {
            return TapDecision::Swallow;
        }
        if event.autorepeat {
            return TapDecision::Pass;
        }
        if self.mode == TapMode::Draining {
            return TapDecision::Pass;
        }
        match event.keycode {
            KEY_RETURN | KEY_KEYPAD_ENTER => {
                // Enter completes only with a trusted frame (Collecting);
                // before that it is held so it cannot mis-touch the target.
                self.held_keys |= bit;
                self.stats.keys_owned += 1;
                if self.mode == TapMode::Collecting && self.command.is_none() {
                    self.command = Some(TapGateCommand::Finish);
                    self.command_key = Some(event.keycode);
                }
                TapDecision::Swallow
            }
            KEY_ESCAPE => {
                // Escape cancels from the moment protection is installed.
                self.held_keys |= bit;
                self.stats.keys_owned += 1;
                if self.command.is_none() {
                    self.command = Some(TapGateCommand::Cancel);
                    self.command_key = Some(event.keycode);
                }
                TapDecision::Swallow
            }
            // Every other key belongs to the target application, including
            // the capture hotkey, which Carbon delivers through its own
            // registration and must keep working during collection.
            _ => TapDecision::Pass,
        }
    }
}

/// The installed Quartz session tap. Main thread only; `Drop` removes it.
#[derive(Debug)]
pub struct ScrollInputTap {
    port: Ref,
    source: Ref,
    gate: Rc<RefCell<TapGate>>,
}

impl ScrollInputTap {
    /// Installs the active session tap in armed mode.
    ///
    /// # Errors
    /// Returns [`ScrollTapError::GestureInProgress`] when a physical button or
    /// an ArcShot-owned key is held (protection must not start mid-gesture) and
    /// [`ScrollTapError::TapUnavailable`] when the system refuses the tap, for
    /// example after Input Monitoring was revoked.
    pub fn install() -> Result<Self, ScrollTapError> {
        MainThreadMarker::new().ok_or(ScrollTapError::NotMainThread)?;
        // SAFETY: These synchronous state queries take a source-state constant
        // and plain button/key indices; no pointers are involved. Only a held
        // physical button refuses installation: its release would otherwise
        // arrive after the tap started and leak to the target. Held keys are
        // deliberately not preflighted — `CGEventSourceKeyState` reflects a
        // sticky global source state that synthetic key-up delivery does not
        // reliably clear, and the gate's press/release ownership already keeps
        // an unowned key-up (or autorepeat) harmless.
        let held_buttons: Vec<u32> = (0..MOUSE_BUTTONS)
            .filter(|button| unsafe { CGEventSourceButtonState(HID_SYSTEM_STATE, *button) })
            .collect();
        if !held_buttons.is_empty() {
            log::info!("scroll_input_gesture_preflight buttons={held_buttons:?}");
            return Err(ScrollTapError::GestureInProgress);
        }
        let gate = Rc::new(RefCell::new(TapGate::armed()));
        let mask = [
            EVENT_LEFT_DOWN,
            EVENT_LEFT_UP,
            EVENT_RIGHT_DOWN,
            EVENT_RIGHT_UP,
            EVENT_LEFT_DRAGGED,
            EVENT_RIGHT_DRAGGED,
            EVENT_KEY_DOWN,
            EVENT_KEY_UP,
            EVENT_SCROLL_WHEEL,
            EVENT_OTHER_DOWN,
            EVENT_OTHER_UP,
            EVENT_OTHER_DRAGGED,
        ]
        .iter()
        .fold(0u64, |mask, kind| mask | (1 << kind));
        // SAFETY: `gate` has a stable shared address for as long as this tap
        // exists; the callback only runs while the tap source is installed on
        // the main run loop, and `Drop` removes it before releasing the gate.
        let port = unsafe {
            CGEventTapCreate(
                SESSION_TAP,
                HEAD_INSERT,
                ACTIVE_FILTER,
                mask,
                tap_callback,
                Rc::as_ptr(&gate).cast::<core::ffi::c_void>().cast_mut(),
            )
        };
        if port.is_null() {
            return Err(ScrollTapError::TapUnavailable);
        }
        // SAFETY: The port is a valid +1 CFMachPort from CGEventTapCreate.
        let source = unsafe { CFMachPortCreateRunLoopSource(ptr::null_mut(), port, 0) };
        if source.is_null() {
            // SAFETY: Partial installation owns this port; no source exists.
            unsafe {
                CFMachPortInvalidate(port);
                CFRelease(port);
            }
            return Err(ScrollTapError::TapUnavailable);
        }
        // SAFETY: GPUI owns the main run loop; adding a source never nests it.
        unsafe {
            CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(port, true);
            if !CGEventTapIsEnabled(port) {
                CFRunLoopRemoveSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
                CFMachPortInvalidate(port);
                CFRelease(source);
                CFRelease(port);
                return Err(ScrollTapError::TapUnavailable);
            }
        }
        Ok(Self { port, source, gate })
    }

    /// Opens collection with the fixed selection in global display points.
    pub fn open_collection(&self, selection: GlobalRect) {
        self.gate.borrow_mut().open_collection(selection);
    }

    /// Re-opens collection after a failed completion attempt.
    pub fn reopen_collection(&self) {
        self.gate.borrow_mut().reopen_collection();
    }

    /// Sets the control window rect before collection opens.
    pub fn set_control_rect(&self, control: GlobalRect) {
        self.gate.borrow_mut().set_control_rect(control);
    }

    /// Ends collection; owned releases keep their ownership until drained.
    pub fn begin_drain(&self) {
        self.gate.borrow_mut().begin_drain();
    }

    /// Whether every owned press has been released and the tap can retire.
    #[must_use]
    pub fn drained(&self) -> bool {
        self.gate.borrow().drained()
    }

    /// Takes the one queued user intent, if any.
    #[must_use]
    pub fn take_command(&self) -> Option<TapGateCommand> {
        self.gate.borrow_mut().take_command()
    }

    /// Whether the system disabled the tap.
    #[must_use]
    pub fn protection_lost(&self) -> bool {
        self.gate.borrow().protection_lost()
    }

    /// The counters reported when the tap is retired.
    #[must_use]
    pub fn stats(&self) -> TapStats {
        self.gate.borrow().stats()
    }
}

impl Drop for ScrollInputTap {
    fn drop(&mut self) {
        // SAFETY: Both references came from this adapter's install calls and
        // are destroyed exactly once, on the main thread, where this source's
        // callbacks run; after removal and invalidation no callback can fire,
        // so releasing the gate's Rc is safe.
        unsafe {
            CGEventTapEnable(self.port, false);
            CFRunLoopRemoveSource(CFRunLoopGetMain(), self.source, kCFRunLoopCommonModes);
            CFMachPortInvalidate(self.port);
            CFRelease(self.source);
            CFRelease(self.port);
        }
    }
}

/// Why the session tap could not be installed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ScrollTapError {
    /// The adapter was used from a non-main thread.
    NotMainThread,
    /// A physical button or ArcShot-owned key was held at install time.
    GestureInProgress,
    /// The system refused or disabled the tap.
    TapUnavailable,
}

impl fmt::Display for ScrollTapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotMainThread => "scroll input tap is main-thread only",
            Self::GestureInProgress => "a button or capture key was held",
            Self::TapUnavailable => "the system refused the scroll input tap",
        })
    }
}
impl Error for ScrollTapError {}

// SAFETY: `info` is the stable `Rc<TapGate>` pointer owned by a live
// `ScrollInputTap`; the callback only runs while that tap is installed, on the
// main thread, and never allocates, blocks, calls AppKit or panics.
unsafe extern "C" fn tap_callback(_proxy: Ref, kind: u32, event: Ref, info: Ref) -> Ref {
    let gate = unsafe { &*(info.cast::<RefCell<TapGate>>()) };
    if matches!(kind, EVENT_TAP_DISABLED_TIMEOUT | EVENT_TAP_DISABLED_INPUT) {
        let _ = gate.borrow_mut().on_event(TapEvent {
            kind: TapEventKind::TapDisabled,
            button: 0,
            click_state: 0,
            keycode: 0,
            autorepeat: false,
            location: [0.0, 0.0],
        });
        return event;
    }
    if event.is_null() {
        return event;
    }
    let translated = unsafe { translate_event(kind, event) };
    let decision = gate.borrow_mut().on_event(translated);
    match decision {
        TapDecision::Pass => event,
        TapDecision::Swallow => NULL_EVENT,
    }
}

/// Reads one CGEvent's public fields into the pure [`TapEvent`].
///
/// # Safety
/// `event` must be a valid borrowed CGEvent for the duration of this call.
unsafe fn translate_event(kind: u32, event: Ref) -> TapEvent {
    // SAFETY: Every field index below is a documented public CGEventField, the
    // values are read by value into plain Rust data, and the location comes
    // from CGEventGetLocation in global display (top-left origin) points —
    // the same coordinate space as CGWindow frames.
    unsafe {
        let point = CGEventGetLocation(event);
        TapEvent {
            kind: match kind {
                EVENT_LEFT_DOWN => TapEventKind::LeftDown,
                EVENT_LEFT_UP => TapEventKind::LeftUp,
                EVENT_RIGHT_DOWN => TapEventKind::RightDown,
                EVENT_RIGHT_UP => TapEventKind::RightUp,
                EVENT_LEFT_DRAGGED | EVENT_RIGHT_DRAGGED | EVENT_OTHER_DRAGGED => {
                    TapEventKind::Dragged
                }
                EVENT_KEY_DOWN => TapEventKind::KeyDown,
                EVENT_KEY_UP => TapEventKind::KeyUp,
                EVENT_SCROLL_WHEEL => TapEventKind::ScrollWheel,
                EVENT_OTHER_DOWN => TapEventKind::OtherDown,
                EVENT_OTHER_UP => TapEventKind::OtherUp,
                _ => TapEventKind::ScrollWheel,
            },
            button: CGEventGetIntegerValueField(event, FIELD_BUTTON_NUMBER).clamp(0, 255) as u8,
            click_state: CGEventGetIntegerValueField(event, FIELD_CLICK_STATE),
            keycode: CGEventGetIntegerValueField(event, FIELD_KEYCODE).clamp(0, i64::from(u16::MAX))
                as u16,
            autorepeat: CGEventGetIntegerValueField(event, FIELD_AUTOREPEAT) != 0,
            location: [point.x, point.y],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SELECTION: GlobalRect = GlobalRect {
        x: 100.0,
        y: 100.0,
        width: 200.0,
        height: 100.0,
    };
    const CONTROL: GlobalRect = GlobalRect {
        x: 600.0,
        y: 500.0,
        width: 60.0,
        height: 40.0,
    };

    fn collecting_gate() -> TapGate {
        let mut gate = TapGate::armed();
        gate.set_control_rect(CONTROL);
        gate.open_collection(SELECTION);
        gate
    }

    fn mouse(kind: TapEventKind, button: u8, at: [f64; 2], click_state: i64) -> TapEvent {
        TapEvent {
            kind,
            button,
            click_state,
            keycode: 0,
            autorepeat: false,
            location: at,
        }
    }

    fn key(kind: TapEventKind, keycode: u16) -> TapEvent {
        TapEvent {
            kind,
            button: 0,
            click_state: 0,
            keycode,
            autorepeat: false,
            location: [0.0, 0.0],
        }
    }

    fn scroll(at: [f64; 2]) -> TapEvent {
        TapEvent {
            kind: TapEventKind::ScrollWheel,
            button: 0,
            click_state: 0,
            keycode: 0,
            autorepeat: false,
            location: at,
        }
    }

    /// MAC-SCROLL-INPUT (gate seam): raw scroll inside the selection passes
    /// through unchanged; outside it, and before the first frame, it is held.
    #[test]
    fn gate_passes_raw_scroll_only_inside_the_open_selection() {
        let mut armed = TapGate::armed();
        assert_eq!(armed.on_event(scroll([150.0, 120.0])), TapDecision::Swallow);

        let mut gate = collecting_gate();
        assert_eq!(gate.on_event(scroll([150.0, 120.0])), TapDecision::Pass);
        assert_eq!(gate.on_event(scroll([99.9, 120.0])), TapDecision::Swallow);
        assert_eq!(gate.on_event(scroll([300.0, 120.0])), TapDecision::Swallow);
        assert_eq!(gate.stats().scrolls_passed, 1);
    }

    /// MAC-SCROLL-INPUT (gate seam): every click press is blocked — including
    /// the first click of a double-click — and its release is owned, so no
    /// stray release ever leaks to the target.
    #[test]
    fn gate_blocks_clicks_and_owns_their_releases() {
        let mut gate = collecting_gate();
        let inside = [150.0, 120.0];
        // First click of a double-click: swallowed, no completion yet.
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftDown, 0, inside, 1)),
            TapDecision::Swallow
        );
        assert_eq!(gate.take_command(), None);
        // A drag with the swallowed button and its release are owned too.
        assert_eq!(
            gate.on_event(mouse(TapEventKind::Dragged, 0, inside, 1)),
            TapDecision::Swallow
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftUp, 0, inside, 1)),
            TapDecision::Swallow
        );
        // Right and other buttons behave the same.
        assert_eq!(
            gate.on_event(mouse(TapEventKind::RightDown, 1, inside, 1)),
            TapDecision::Swallow
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::RightUp, 1, inside, 1)),
            TapDecision::Swallow
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::OtherDown, 2, inside, 1)),
            TapDecision::Swallow
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::OtherUp, 2, inside, 1)),
            TapDecision::Swallow
        );
        assert_eq!(gate.stats().clicks_blocked, 3);
        // A release whose press we never owned passes normally.
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftUp, 3, inside, 1)),
            TapDecision::Pass
        );
    }

    /// MAC-SCROLL-INPUT (gate seam): the second click of a double-click
    /// anywhere outside the control window completes collection.
    #[test]
    fn gate_double_click_completes_from_the_second_click() {
        let mut gate = collecting_gate();
        let inside = [150.0, 120.0];
        let _ = gate.on_event(mouse(TapEventKind::LeftDown, 0, inside, 1));
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftDown, 0, inside, 2)),
            TapDecision::Swallow
        );
        assert_eq!(gate.take_command(), Some(TapGateCommand::Finish));
        // A double-click on ArcShot's own control window is UI, not completion.
        let mut gate = collecting_gate();
        let on_control = [620.0, 510.0];
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftDown, 0, on_control, 1)),
            TapDecision::Pass
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftDown, 0, on_control, 2)),
            TapDecision::Pass
        );
        assert_eq!(gate.take_command(), None);
        // Before the trusted first frame a double-click completes nothing.
        let mut armed = TapGate::armed();
        let _ = armed.on_event(mouse(TapEventKind::LeftDown, 0, inside, 1));
        let _ = armed.on_event(mouse(TapEventKind::LeftDown, 0, inside, 2));
        assert_eq!(armed.take_command(), None);
    }

    /// MAC-SCROLL-INPUT (gate seam): Enter and Escape belong to ArcShot with
    /// matched press/release ownership; other keys and the hotkey pass through.
    #[test]
    fn gate_owns_enter_and_escape_with_matched_releases() {
        let mut gate = collecting_gate();
        // Enter completes; its keyUp stays owned.
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyDown, 36)),
            TapDecision::Swallow
        );
        assert_eq!(gate.take_command(), Some(TapGateCommand::Finish));
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyUp, 36)),
            TapDecision::Swallow
        );
        // Keypad Enter completes too.
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyDown, 76)),
            TapDecision::Swallow
        );
        assert_eq!(gate.take_command(), Some(TapGateCommand::Finish));
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyUp, 76)),
            TapDecision::Swallow
        );
        // Escape cancels from armed state, not only while collecting.
        let mut armed = TapGate::armed();
        assert_eq!(
            armed.on_event(key(TapEventKind::KeyDown, 53)),
            TapDecision::Swallow
        );
        assert_eq!(armed.take_command(), Some(TapGateCommand::Cancel));
        assert_eq!(
            armed.on_event(key(TapEventKind::KeyUp, 53)),
            TapDecision::Swallow
        );
        // Other keys pass in both directions: typing and the capture hotkey
        // (⌘⇧X here) keep working in the target application.
        for keycode in [0u16, 7, 45] {
            assert_eq!(
                gate.on_event(key(TapEventKind::KeyDown, keycode)),
                TapDecision::Pass
            );
            assert_eq!(
                gate.on_event(key(TapEventKind::KeyUp, keycode)),
                TapDecision::Pass
            );
        }
        // Autorepeats of owned keys stay owned and never re-command.
        let _ = gate.on_event(key(TapEventKind::KeyDown, 36));
        let _ = gate.take_command();
        let mut repeat = key(TapEventKind::KeyDown, 36);
        repeat.autorepeat = true;
        assert_eq!(gate.on_event(repeat), TapDecision::Swallow);
        assert_eq!(gate.take_command(), None);
        assert_eq!(gate.stats().keys_owned, 3);
    }

    /// MAC-LIFETIME-PRIVACY (gate seam): draining releases new gestures to the
    /// user while still swallowing the releases of input ArcShot owns, and the
    /// tap is retired only when every owned press has been released.
    #[test]
    fn drain_keeps_release_only_interception_until_owned_input_releases() {
        let mut gate = collecting_gate();
        // The user holds a mouse button (owned) and then collection ends.
        let _ = gate.on_event(mouse(TapEventKind::LeftDown, 0, [150.0, 120.0], 1));
        gate.begin_drain();
        assert!(!gate.drained());
        // New input during the drain belongs to the user again.
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyDown, 0)),
            TapDecision::Pass
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::RightDown, 1, [150.0, 120.0], 1)),
            TapDecision::Pass
        );
        assert_eq!(gate.on_event(scroll([150.0, 120.0])), TapDecision::Pass);
        // The owned left-button release is still swallowed; then the drain ends.
        assert_eq!(
            gate.on_event(mouse(TapEventKind::LeftUp, 0, [150.0, 120.0], 1)),
            TapDecision::Swallow
        );
        assert_eq!(
            gate.on_event(mouse(TapEventKind::RightUp, 1, [150.0, 120.0], 1)),
            TapDecision::Pass
        );
        assert!(gate.drained());
    }

    /// A command's own key press is consumed at drain time: synthetic key-up
    /// deliveries are not guaranteed, so the tap must not stay installed for a
    /// key whose intent ArcShot already handled.
    #[test]
    fn consumed_command_key_does_not_block_the_drain() {
        let mut gate = collecting_gate();
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyDown, 36)),
            TapDecision::Swallow
        );
        assert_eq!(gate.take_command(), Some(TapGateCommand::Finish));
        gate.begin_drain();
        assert!(gate.drained());
        // No key-up arrives; the native tap can retire immediately.
    }

    /// MAC-TARGET-FAIL (gate seam): a system-disabled tap is explicitly lost
    /// protection, never a successful drain or an ordinary failure.
    #[test]
    fn tap_disabled_by_the_system_is_lost_protection() {
        let mut gate = collecting_gate();
        let _ = gate.on_event(TapEvent {
            kind: TapEventKind::TapDisabled,
            button: 0,
            click_state: 0,
            keycode: 0,
            autorepeat: false,
            location: [0.0, 0.0],
        });
        assert_eq!(gate.take_command(), Some(TapGateCommand::ProtectionLost));
        assert!(!gate.drained());
        assert!(gate.protection_lost());
    }

    /// MAC-SCROLL-INPUT (gate seam): a failed completion re-arms collection so
    /// Enter can retry without reinstalling the tap.
    #[test]
    fn gate_reopens_collection_after_a_failed_completion() {
        let mut gate = collecting_gate();
        gate.begin_drain();
        gate.reopen_collection();
        // (reopen returns nothing; the pass-through check below is the point.)
        assert_eq!(gate.on_event(scroll([150.0, 120.0])), TapDecision::Pass);
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyDown, 36)),
            TapDecision::Swallow
        );
        assert_eq!(gate.take_command(), Some(TapGateCommand::Finish));
    }

    /// Only one user intent is queued; later intents inside the same poll
    /// interval do not overwrite the first one.
    #[test]
    fn gate_queues_only_the_first_intent() {
        let mut gate = collecting_gate();
        let _ = gate.on_event(key(TapEventKind::KeyDown, 36));
        let _ = gate.on_event(key(TapEventKind::KeyDown, 53));
        assert_eq!(gate.take_command(), Some(TapGateCommand::Finish));
        assert_eq!(gate.take_command(), None);
        // Both keys are still owned for release matching.
        assert_eq!(
            gate.on_event(key(TapEventKind::KeyUp, 53)),
            TapDecision::Swallow
        );
    }

    /// An untracked button number is treated as lost protection rather than
    /// silently owning something the gate cannot represent.
    #[test]
    fn gate_treats_untracked_buttons_as_lost_protection() {
        let mut gate = collecting_gate();
        let decision = gate.on_event(mouse(TapEventKind::LeftDown, 200, [150.0, 120.0], 1));
        assert_eq!(decision, TapDecision::Pass);
        assert_eq!(gate.take_command(), Some(TapGateCommand::ProtectionLost));
    }
}
