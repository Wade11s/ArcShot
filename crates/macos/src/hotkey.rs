//! Global capture hotkey registration and persistence, main thread only.
//!
//! Plain-key global hotkeys use Carbon HIToolbox `RegisterEventHotKey` on the
//! application event target, exactly the path the research branch verified in
//! the macOS 27 SDK (`CarbonEvents.h`). Registration is non-exclusive; success
//! only means this process will be notified, never that the combination is
//! system-unique. No event tap is created, so no Accessibility or Input
//! Monitoring permission is required or requested.

use std::{cell::RefCell, error::Error, fmt, ptr, rc::Rc};

use arcshot_core::Hotkey;
use objc2::MainThreadMarker;
use objc2_foundation::{NSString, NSUserDefaults};

type CVoid = core::ffi::c_void;
type OSStatus = i32;
type EventTargetRef = *mut CVoid;
type EventHotKeyRef = *mut CVoid;
type EventHandlerRef = *mut CVoid;
type EventHandlerCallRef = *mut CVoid;
type EventRef = *mut CVoid;

/// `EventHotKeyID`: distinguishes our registrations inside Carbon events.
#[repr(C)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

/// `EventTypeSpec`: the event classes the handler wants.
#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

// HIToolbox constants from the macOS 27 SDK headers.
const K_EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
const K_EVENT_PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
const TYPE_EVENT_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");
const CMD_KEY: u32 = 0x0100;
const SHIFT_KEY: u32 = 0x0200;
const OPTION_KEY: u32 = 0x0800;
const CONTROL_KEY: u32 = 0x1000;
const NO_ERR: OSStatus = 0;
const EVENT_NOT_HANDLED_ERR: OSStatus = -9874;
/// Marks hot key events belonging to this adapter.
const HOTKEY_SIGNATURE: u32 = u32::from_be_bytes(*b"ASHK");

// SAFETY: These HIToolbox C entry points keep their classical signatures; the
// framework is linked directly and every pointer originates in this module.
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        in_target: EventTargetRef,
        in_handler: unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut CVoid) -> OSStatus,
        in_num_types: usize,
        in_types: *const EventTypeSpec,
        in_user_data: *mut CVoid,
        out_ref: *mut EventHandlerRef,
    ) -> OSStatus;
    fn RemoveEventHandler(in_user_handler: EventHandlerRef) -> OSStatus;
    fn RegisterEventHotKey(
        in_key_code: u32,
        in_modifiers: u32,
        in_hot_key_id: EventHotKeyID,
        in_target: EventTargetRef,
        in_options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(in_hot_key: EventHotKeyRef) -> OSStatus;
    fn GetEventParameter(
        in_event: EventRef,
        in_name: u32,
        in_desired_type: u32,
        out_actual_type: *mut u32,
        in_buffer_size: usize,
        out_actual_size: *mut usize,
        out_data: *mut CVoid,
    ) -> OSStatus;
}

/// Carbon virtual keycodes (`kVK_ANSI_*`) for the ANSI printable keys.
#[must_use]
pub fn virtual_keycode(key: char) -> Option<u32> {
    Some(match key.to_ascii_lowercase() {
        'a' => 0x00,
        's' => 0x01,
        'd' => 0x02,
        'f' => 0x03,
        'h' => 0x04,
        'g' => 0x05,
        'z' => 0x06,
        'x' => 0x07,
        'c' => 0x08,
        'v' => 0x09,
        'b' => 0x0B,
        'q' => 0x0C,
        'w' => 0x0D,
        'e' => 0x0E,
        'r' => 0x0F,
        'y' => 0x10,
        't' => 0x11,
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '6' => 0x16,
        '5' => 0x17,
        '=' => 0x18,
        '9' => 0x19,
        '7' => 0x1A,
        '-' => 0x1B,
        '8' => 0x1C,
        '0' => 0x1D,
        ']' => 0x1E,
        'o' => 0x1F,
        'u' => 0x20,
        '[' => 0x21,
        'i' => 0x22,
        'p' => 0x23,
        'l' => 0x25,
        'j' => 0x26,
        '\'' => 0x27,
        'k' => 0x28,
        ';' => 0x29,
        '\\' => 0x2A,
        ',' => 0x2B,
        '/' => 0x2C,
        'n' => 0x2D,
        'm' => 0x2E,
        '.' => 0x2F,
        ' ' => 0x31,
        '`' => 0x32,
        _ => return None,
    })
}

fn carbon_modifiers(hotkey: Hotkey) -> u32 {
    let mut modifiers = 0;
    if hotkey.command() {
        modifiers |= CMD_KEY;
    }
    if hotkey.shift() {
        modifiers |= SHIFT_KEY;
    }
    if hotkey.option() {
        modifiers |= OPTION_KEY;
    }
    if hotkey.control() {
        modifiers |= CONTROL_KEY;
    }
    modifiers
}

// SAFETY: `user_data` is the press counter owned (as `Rc`) by `GlobalHotkeys`;
// the handler is removed before that owner can drop, so the pointer is valid
// for every invocation. The counter and this handler both run on the main
// thread only, so the `RefCell` cannot race.
unsafe extern "C" fn hot_key_pressed(
    _call: EventHandlerCallRef,
    event: EventRef,
    user_data: *mut CVoid,
) -> OSStatus {
    let presses = unsafe { &*(user_data.cast::<RefCell<u32>>()) };
    let mut identifier = EventHotKeyID {
        signature: 0,
        id: 0,
    };
    let status = unsafe {
        GetEventParameter(
            event,
            K_EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            ptr::null_mut(),
            size_of::<EventHotKeyID>(),
            ptr::null_mut(),
            ptr::from_mut(&mut identifier).cast(),
        )
    };
    if status == NO_ERR && identifier.signature == HOTKEY_SIGNATURE {
        *presses.borrow_mut() += 1;
        NO_ERR
    } else {
        EVENT_NOT_HANDLED_ERR
    }
}

/// The process-wide application-event-target hotkey adapter.
///
/// All methods must run on the main thread. At most one combination is
/// registered at a time; `suspend` unregisters it without forgetting the
/// value so `resume` can restore it.
pub struct GlobalHotkeys {
    handler: EventHandlerRef,
    presses: Rc<RefCell<u32>>,
    active: Option<(Hotkey, EventHotKeyRef)>,
    suspended: Option<Hotkey>,
    /// References whose unregistration failed; retried on drop.
    retired: Vec<EventHotKeyRef>,
}

impl GlobalHotkeys {
    /// Installs the shared hot key event handler on the application target.
    ///
    /// # Errors
    /// Returns an error when called off the main thread or when HIToolbox
    /// refuses the handler installation.
    pub fn install() -> Result<Self, HotkeyError> {
        let _marker = MainThreadMarker::new().ok_or(HotkeyError::NotMainThread)?;
        let presses = Rc::new(RefCell::new(0));
        let spec = EventTypeSpec {
            event_class: K_EVENT_CLASS_KEYBOARD,
            event_kind: K_EVENT_HOT_KEY_PRESSED,
        };
        let mut handler: EventHandlerRef = ptr::null_mut();
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                hot_key_pressed,
                1,
                &spec,
                Rc::as_ptr(&presses).cast_mut().cast::<CVoid>(),
                ptr::from_mut(&mut handler),
            )
        };
        if status != NO_ERR || handler.is_null() {
            return Err(HotkeyError::Install(status));
        }
        Ok(Self {
            handler,
            presses,
            active: None,
            suspended: None,
            retired: Vec::new(),
        })
    }

    /// Registers `hotkey`, replacing any previous registration only after the
    /// new one succeeded, so a refusal leaves the old registration untouched.
    ///
    /// # Errors
    /// Returns an error off the main thread, for keys without an ANSI virtual
    /// keycode, or when the system refuses the registration.
    pub fn set_active(&mut self, hotkey: Hotkey) -> Result<(), HotkeyError> {
        self.main_thread()?;
        let reference = self.register(hotkey)?;
        if let Some((_, previous)) = self.active.take() {
            // Best effort: the new registration is already active; a failed
            // unregister only leaves the replaced combination also firing,
            // and the reference is retried on drop.
            if let Err(error) = self.unregister(previous) {
                log::error!("hotkey_replace_unregister_failure={error}");
                self.retired.push(previous);
            }
        }
        self.active = Some((hotkey, reference));
        self.suspended = None;
        Ok(())
    }

    /// Unregisters the active combination while remembering its value.
    ///
    /// # Errors
    /// Returns an error off the main thread or when unregistration fails; in
    /// that case the registration is still active and `suspended` is empty.
    pub fn suspend(&mut self) -> Result<(), HotkeyError> {
        self.main_thread()?;
        if let Some((hotkey, reference)) = self.active.take() {
            if let Err(error) = self.unregister(reference) {
                self.active = Some((hotkey, reference));
                return Err(error);
            }
            self.suspended = Some(hotkey);
        }
        Ok(())
    }

    /// Re-registers the value remembered by [`GlobalHotkeys::suspend`].
    ///
    /// # Errors
    /// Returns an error off the main thread or when re-registration fails;
    /// the remembered value is kept so callers must report it as not active.
    pub fn resume(&mut self) -> Result<(), HotkeyError> {
        self.main_thread()?;
        let Some(hotkey) = self.suspended else {
            return Ok(());
        };
        let reference = self.register(hotkey)?;
        self.active = Some((hotkey, reference));
        self.suspended = None;
        Ok(())
    }

    /// The value remembered while suspended, if any.
    #[must_use]
    pub const fn suspended(&self) -> Option<Hotkey> {
        self.suspended
    }

    /// Whether a press was delivered since the previous drain.
    #[must_use]
    pub fn take_pressed(&self) -> bool {
        let mut presses = self.presses.borrow_mut();
        let pressed = *presses > 0;
        *presses = 0;
        pressed
    }

    fn register(&self, hotkey: Hotkey) -> Result<EventHotKeyRef, HotkeyError> {
        let Some(keycode) = virtual_keycode(hotkey.key()) else {
            return Err(HotkeyError::UnsupportedKey);
        };
        let identifier = EventHotKeyID {
            signature: HOTKEY_SIGNATURE,
            id: 1,
        };
        let mut reference: EventHotKeyRef = ptr::null_mut();
        let status = unsafe {
            RegisterEventHotKey(
                keycode,
                carbon_modifiers(hotkey),
                identifier,
                GetApplicationEventTarget(),
                0,
                ptr::from_mut(&mut reference),
            )
        };
        if status == NO_ERR && !reference.is_null() {
            Ok(reference)
        } else {
            Err(HotkeyError::Registration(status))
        }
    }

    fn unregister(&self, reference: EventHotKeyRef) -> Result<(), HotkeyError> {
        if reference.is_null() {
            return Ok(());
        }
        let status = unsafe { UnregisterEventHotKey(reference) };
        if status == NO_ERR {
            Ok(())
        } else {
            Err(HotkeyError::Unregistration(status))
        }
    }

    fn main_thread(&self) -> Result<(), HotkeyError> {
        if MainThreadMarker::new().is_some() {
            Ok(())
        } else {
            Err(HotkeyError::NotMainThread)
        }
    }
}

impl Drop for GlobalHotkeys {
    fn drop(&mut self) {
        // SAFETY: both references were produced by the register/install calls
        // above and are removed exactly once, before `presses` can drop.
        unsafe {
            if let Some((_, reference)) = self.active.take() {
                let _ = UnregisterEventHotKey(reference);
            }
            for reference in self.retired.drain(..) {
                let _ = UnregisterEventHotKey(reference);
            }
            if !self.handler.is_null() {
                let status = RemoveEventHandler(self.handler);
                if status != NO_ERR {
                    // The handler may still be installed; leak the counter it
                    // borrows instead of freeing memory it could write to.
                    log::error!("{}", HotkeyError::Remove(status));
                    std::mem::forget(Rc::clone(&self.presses));
                }
            }
        }
    }
}

impl fmt::Debug for GlobalHotkeys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GlobalHotkeys")
            .field("active", &self.active.map(|(hotkey, _)| hotkey))
            .field("suspended", &self.suspended)
            .finish_non_exhaustive()
    }
}

const HOTKEY_STORAGE_KEY: &str = "GlobalCaptureHotkey";

/// Loads the persisted hotkey; malformed or missing values fall back to the
/// product default.
pub fn hotkey_settings_load() -> Hotkey {
    NSUserDefaults::standardUserDefaults()
        .stringForKey(&NSString::from_str(HOTKEY_STORAGE_KEY))
        .map(|stored| stored.to_string())
        .and_then(|stored| Hotkey::from_storage_string(&stored))
        .unwrap_or_default()
}

/// Persists the hotkey configuration locally.
pub fn hotkey_settings_store(hotkey: Hotkey) {
    // SAFETY: `NSString` is an `NSObject`; the value and key outlive the call.
    unsafe {
        NSUserDefaults::standardUserDefaults().setObject_forKey(
            Some(&NSString::from_str(&hotkey.to_storage_string())),
            &NSString::from_str(HOTKEY_STORAGE_KEY),
        );
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum HotkeyError {
    /// The adapter was used from a non-main thread.
    NotMainThread,
    /// The application-target handler could not be installed.
    Install(OSStatus),
    /// The system refused a registration.
    Registration(OSStatus),
    /// Unregistering a previously registered reference failed.
    Unregistration(OSStatus),
    /// Removing the event handler failed.
    Remove(OSStatus),
    /// The key has no ANSI virtual keycode on this system.
    UnsupportedKey,
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotMainThread => "hotkey adapter is main-thread only".to_owned(),
            Self::Install(status) => format!("could not install hotkey handler ({status})"),
            Self::Registration(status) => format!("the system refused the hotkey ({status})"),
            Self::Unregistration(status) => format!("could not unregister the hotkey ({status})"),
            Self::Remove(status) => format!("could not remove the hotkey handler ({status})"),
            Self::UnsupportedKey => "the key has no ANSI virtual keycode".to_owned(),
        };
        formatter.write_str(&message)
    }
}

impl Error for HotkeyError {}
