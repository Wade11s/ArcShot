//! Safe macOS adapters for ArcShot.

mod capture;
mod clipboard;
mod hotkey;
mod login_item;
mod permission;
mod preferences;
mod save;
mod scroll;

pub use capture::{
    CaptureError, DisplaySnapshot, NativeCapture, builtin_display, capture_builtin,
    owned_visible_window_ids,
};
pub use clipboard::{
    ClipboardError, ClipboardSmoke, play_completion_sound, validate_png_pasteboard,
    write_png_to_clipboard,
};
pub use hotkey::{
    GlobalHotkeys, HotkeyError, hotkey_settings_load, hotkey_settings_store, virtual_keycode,
};
pub use login_item::{LoginItemError, login_item_status, set_login_item_enabled};
pub use permission::{
    PermissionError, mark_permission_guide_seen, open_screen_recording_settings,
    permission_guide_seen, request_screen_recording, screen_recording_available,
};
pub use preferences::{
    DensityError, completion_sound_enabled, density, store_completion_sound, store_density,
};
pub use save::{
    PngSavePanel, SaveFileError, SavePanelError, SavePanelOutcome, cancel_save_panel,
    take_save_panel_outcome, write_png_to_file,
};
pub use scroll::{
    frontmost_pid, open_scroll_permission_settings, pointer_location_cg, probe_scroll_target,
    request_scroll_permission, scroll_exclusion_set, scroll_permissions,
};

use std::{cell::RefCell, error::Error, fmt};

use arcshot_core::{Density, LoginItemState, MenuBarText, StatusItemIcon};
use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSControlStateValue, NSControlStateValueOff,
    NSControlStateValueOn, NSImage, NSMenu, NSMenuItem, NSSquareStatusItemLength, NSStatusBar,
    NSStatusItem,
};
use objc2_foundation::{NSData, NSObject, NSSize, NSString};

/// Whether this application is active. Returns false when called off the main thread.
#[must_use]
pub fn application_active() -> bool {
    MainThreadMarker::new()
        .is_some_and(|marker| NSApplication::sharedApplication(marker).isActive())
}

/// Commands queued by native menu actions for later consumption by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCommand {
    /// Start a capture session.
    StartCapture,
    /// Open the capture hotkey settings window.
    HotkeySettings,
    /// Show the Screen Recording permission flow.
    ScreenRecordingPermissions,
    /// Switch 选项密度 between Standard and Compact.
    ToggleDensity,
    /// Toggle the persisted 完成音效 preference.
    ToggleCompletionSound,
    /// Ask the system to enable or disable 登录时启动.
    ToggleLaunchAtLogin,
}

/// Tags identify the three preference items inside `validateMenuItem:`.
const COMPLETION_SOUND_ITEM_TAG: isize = 1;
const LAUNCH_AT_LOGIN_ITEM_TAG: isize = 2;
const DENSITY_ITEM_TAG: isize = 3;

fn item_state(enabled: bool) -> NSControlStateValue {
    if enabled {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class has no Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Vec<MenuCommand>>]
    struct CommandTarget;

    impl CommandTarget {
        // SAFETY: AppKit action methods accept a single sender argument.
        #[unsafe(method(startCapture:))]
        fn start_capture(&self, _sender: &NSObject) {
            self.ivars().borrow_mut().push(MenuCommand::StartCapture);
        }

        // SAFETY: AppKit action methods accept a single sender argument.
        #[unsafe(method(hotkeySettings:))]
        fn hotkey_settings(&self, _sender: &NSObject) {
            self.ivars().borrow_mut().push(MenuCommand::HotkeySettings);
        }
        // SAFETY: AppKit action methods accept a single sender argument.
        #[unsafe(method(screenRecordingPermissions:))]
        fn permissions(&self, _sender: &NSObject) {
            self.ivars().borrow_mut().push(MenuCommand::ScreenRecordingPermissions);
        }

        // SAFETY: AppKit action methods accept a single sender argument.
        #[unsafe(method(toggleDensity:))]
        fn toggle_density(&self, _sender: &NSObject) {
            self.ivars().borrow_mut().push(MenuCommand::ToggleDensity);
        }

        // SAFETY: AppKit action methods accept a single sender argument.
        #[unsafe(method(toggleCompletionSound:))]
        fn toggle_completion_sound(&self, _sender: &NSObject) {
            self.ivars().borrow_mut().push(MenuCommand::ToggleCompletionSound);
        }

        // SAFETY: AppKit action methods accept a single sender argument.
        #[unsafe(method(toggleLaunchAtLogin:))]
        fn toggle_launch_at_login(&self, _sender: &NSObject) {
            self.ivars().borrow_mut().push(MenuCommand::ToggleLaunchAtLogin);
        }

        // AppKit sends this to the item target while the status menu opens, so
        // the three preference items always show the stored values and the real
        // login-item status instead of the last requested one. A failed density
        // read is logged and leaves the last known state untouched.
        //
        // SAFETY: `NSMenuItem` is AppKit's validator argument type; this only
        // reads local preferences and ServiceManagement status.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            match item.tag() {
                COMPLETION_SOUND_ITEM_TAG => {
                    item.setState(item_state(completion_sound_enabled()));
                }
                LAUNCH_AT_LOGIN_ITEM_TAG => {
                    let state = login_item_status();
                    log::info!("login_item_status state={state:?} source=menu");
                    item.setState(item_state(state == LoginItemState::Enabled));
                }
                DENSITY_ITEM_TAG => match density() {
                    Ok(current) => item.setState(item_state(current == Density::Standard)),
                    Err(error) => log::warn!("density_menu_read_failure={error}"),
                },
                _ => {}
            }
            // 开始截图 / 截图快捷键 / 屏幕录制权限 stay usable; validation must
            // never disable a command just because it is not a preference item.
            true
        }
    }
);

impl CommandTarget {
    fn new(marker: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(marker).set_ivars(RefCell::new(Vec::new()));
        // SAFETY: NSObject's init consumes the allocated object and returns it retained.
        unsafe { msg_send![super(this), init] }
    }
}

/// Standard status-item point size; the 36-pixel asset renders at 2x without scaling.
const STATUS_ITEM_SIDE_POINTS: f64 = 18.0;

/// A native menu-bar status item whose resources are owned on the main thread.
pub struct MenuBar {
    marker: MainThreadMarker,
    status_bar: Retained<NSStatusBar>,
    status_item: Retained<NSStatusItem>,
    density_item: Retained<NSMenuItem>,
    completion_sound_item: Retained<NSMenuItem>,
    launch_at_login_item: Retained<NSMenuItem>,
    quit_item: Retained<NSMenuItem>,
    command_target: Retained<CommandTarget>,
}

impl MenuBar {
    /// Creates the status item, command queue, and Quit ArcShot item.
    ///
    /// GPUI's crates.io macOS backend selects the Regular activation policy immediately
    /// before its launch callback. This adapter switches back to Accessory before exposing
    /// the status item. The app bundle also sets `LSUIElement` for LaunchServices startup.
    ///
    /// # Errors
    ///
    /// Returns an error when called off the main thread, when Accessory activation cannot
    /// be established, or when AppKit cannot construct the expected native object graph.
    pub fn try_new(icon: StatusItemIcon, text: MenuBarText) -> Result<Self, MenuBarError> {
        let marker = MainThreadMarker::new().ok_or(MenuBarError::NotMainThread)?;
        let application = NSApplication::sharedApplication(marker);
        if !application.setActivationPolicy(NSApplicationActivationPolicy::Accessory)
            && application.activationPolicy() != NSApplicationActivationPolicy::Accessory
        {
            return Err(MenuBarError::ActivationPolicy);
        }

        let data = NSData::with_bytes(icon.png_bytes());
        let image = NSImage::initWithData(NSImage::alloc(), &data)
            .ok_or(MenuBarError::InvalidStatusIcon)?;
        image.setSize(NSSize::new(
            STATUS_ITEM_SIDE_POINTS,
            STATUS_ITEM_SIDE_POINTS,
        ));
        image.setTemplate(true);
        image.setAccessibilityDescription(Some(&NSString::from_str(text.tooltip())));

        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(NSSquareStatusItemLength);
        let button = status_item
            .button(marker)
            .ok_or(MenuBarError::MissingButton)?;
        button.setImage(Some(&image));
        button.setToolTip(Some(&NSString::from_str(text.tooltip())));

        let menu =
            NSMenu::initWithTitle(NSMenu::alloc(marker), &NSString::from_str(text.tooltip()));
        let command_target = CommandTarget::new(marker);
        let mut completed_items = Vec::new();
        for (title, action, tag) in [
            ("开始截图", sel!(startCapture:), 0),
            ("截图快捷键…", sel!(hotkeySettings:), 0),
            ("屏幕录制权限…", sel!(screenRecordingPermissions:), 0),
            ("选项密度", sel!(toggleDensity:), DENSITY_ITEM_TAG),
            (
                "完成音效",
                sel!(toggleCompletionSound:),
                COMPLETION_SOUND_ITEM_TAG,
            ),
            (
                "登录时启动",
                sel!(toggleLaunchAtLogin:),
                LAUNCH_AT_LOGIN_ITEM_TAG,
            ),
        ] {
            // SAFETY: All six selectors are implemented by CommandTarget with
            // the AppKit action signature. MenuBar retains the target until
            // menu removal.
            let item = unsafe {
                let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(marker),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(""),
                );
                item.setTarget(Some(&command_target));
                item.setTag(tag);
                item
            };
            menu.addItem(&item);
            completed_items.push(item);
        }
        let [
            _,
            _,
            _,
            density_item,
            completion_sound_item,
            launch_at_login_item,
        ] = completed_items.as_slice()
        else {
            return Err(MenuBarError::MissingPreferenceItems);
        };
        let density = density().unwrap_or_else(|error| {
            log::warn!("density_launch_read_failure={error}");
            Density::default()
        });
        // Standard shows the checkmark; Compact (the default) leaves it off.
        density_item.setState(item_state(density == Density::Standard));
        completion_sound_item.setState(item_state(completion_sound_enabled()));
        launch_at_login_item.setState(item_state(login_item_status() == LoginItemState::Enabled));
        // SAFETY: `terminate:` is an AppKit-declared selector with the required
        // target/action signature. The item uses the responder chain as its target.
        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(marker),
                &NSString::from_str(text.quit_item()),
                Some(sel!(terminate:)),
                &NSString::from_str("q"),
            )
        };
        menu.addItem(&quit_item);
        status_item.setMenu(Some(&menu));
        status_item.setVisible(true);

        let menu_bar = Self {
            marker,
            status_bar,
            status_item,
            density_item: density_item.clone(),
            completion_sound_item: completion_sound_item.clone(),
            launch_at_login_item: launch_at_login_item.clone(),
            quit_item,
            command_target,
        };
        menu_bar.validate()?;
        Ok(menu_bar)
    }

    /// Hides the status item before enumerating windows for capture.
    ///
    /// This prevents its menu reopening but does not dismiss an already-open menu.
    pub fn hide_for_capture(&self) {
        self.status_item.setVisible(false);
    }

    /// Restores the status item after the capture worker result or watchdog timeout.
    /// This does not cancel a native request or authorize a retry.
    pub fn restore_after_capture(&self) {
        self.status_item.setVisible(true);
    }

    /// Drains commands queued by AppKit actions, without invoking app callbacks.
    pub fn drain_commands(&self) -> Vec<MenuCommand> {
        std::mem::take(&mut *self.command_target.ivars().borrow_mut())
    }

    /// Reflects a completed 完成音效 toggle in the native item state.
    pub fn set_completion_sound_state(&self, enabled: bool) {
        self.completion_sound_item.setState(item_state(enabled));
    }

    /// Reflects the confirmed 选项密度 in the native item state.
    ///
    /// Standard is checked, Compact is not: this checkmark is the AX-observable
    /// menu state for the real current density. It is only set from a confirmed
    /// write or a real read, never from an unconfirmed intent.
    pub fn set_density_state(&self, density: Density) {
        self.density_item
            .setState(item_state(density == Density::Standard));
    }

    /// Reflects an actual login-item read in the native item state.
    ///
    /// Only [`LoginItemState::Enabled`] shows the item as on; a registered item
    /// still waiting for approval is not a working login item.
    pub fn set_launch_at_login_state(&self, state: LoginItemState) {
        self.launch_at_login_item
            .setState(item_state(state == LoginItemState::Enabled));
    }

    /// Checks the observable native startup invariants without opening a window.
    ///
    /// # Errors
    ///
    /// Returns an error if the application is not an Accessory app or the status item,
    /// image, menu, and quit action are not in their required state.
    pub fn validate(&self) -> Result<(), MenuBarError> {
        let application = NSApplication::sharedApplication(self.marker);
        if application.activationPolicy() != NSApplicationActivationPolicy::Accessory {
            return Err(MenuBarError::ActivationPolicy);
        }
        if !self.status_item.isVisible() {
            return Err(MenuBarError::StatusItemNotVisible);
        }
        let button = self
            .status_item
            .button(self.marker)
            .ok_or(MenuBarError::MissingButton)?;
        if button.image().is_none() {
            return Err(MenuBarError::MissingStatusImage);
        }
        if self.status_item.menu(self.marker).is_none() {
            return Err(MenuBarError::MissingMenu);
        }
        if self.quit_item.action() != Some(sel!(terminate:)) {
            return Err(MenuBarError::MissingQuitAction);
        }
        Ok(())
    }

    /// Exercises all six status-menu commands without starting a capture.
    ///
    /// The startup smoke process has no coordinator consuming commands, so the
    /// preference actions only queue their command and never touch the system.
    /// This checks AppKit target/action delivery and command ordering, not
    /// ServiceManagement registration, permissions or SCK.
    ///
    /// # Errors
    /// Returns an error if a menu action is missing, cannot be sent, or queues wrongly.
    pub fn validate_capture_commands(&self) -> Result<(), MenuBarError> {
        self.validate()?;
        if !self.drain_commands().is_empty() {
            return Err(MenuBarError::CaptureCommands);
        }
        let menu = self
            .status_item
            .menu(self.marker)
            .ok_or(MenuBarError::MissingMenu)?;
        // Six commands plus 退出 ArcShot: the I-04 menu has exactly seven items.
        if menu.numberOfItems() != 7 {
            return Err(MenuBarError::CaptureCommands);
        }
        let application = NSApplication::sharedApplication(self.marker);
        for (index, expected) in [
            (0, sel!(startCapture:)),
            (1, sel!(hotkeySettings:)),
            (2, sel!(screenRecordingPermissions:)),
            (3, sel!(toggleDensity:)),
            (4, sel!(toggleCompletionSound:)),
            (5, sel!(toggleLaunchAtLogin:)),
        ] {
            let item = menu
                .itemAtIndex(index)
                .ok_or(MenuBarError::CaptureCommands)?;
            if item.action() != Some(expected) {
                return Err(MenuBarError::CaptureCommands);
            }
            let target = item.target().ok_or(MenuBarError::CaptureCommands)?;
            // SAFETY: The expected selectors are checked above and implemented by
            // our retained CommandTarget. Its actions only enqueue enum values.
            if !unsafe { application.sendAction_to_from(expected, Some(&target), Some(&item)) } {
                return Err(MenuBarError::CaptureCommands);
            }
        }
        // The three preference actions are queued but not executed in smoke mode.
        if self.drain_commands()
            != [
                MenuCommand::StartCapture,
                MenuCommand::HotkeySettings,
                MenuCommand::ScreenRecordingPermissions,
                MenuCommand::ToggleDensity,
                MenuCommand::ToggleCompletionSound,
                MenuCommand::ToggleLaunchAtLogin,
            ]
        {
            return Err(MenuBarError::CaptureCommands);
        }
        Ok(())
    }

    /// Checks the three preference items' selectors and their state against the
    /// real stored preferences and the actual ServiceManagement login-item status.
    ///
    /// Returns the real login-item state for the caller's evidence line. This is
    /// a read only: the startup smoke never registers or unregisters a login item.
    ///
    /// # Errors
    /// Returns an error when a preference item is missing, has the wrong action,
    /// a density value cannot be read, or a state disagrees with the actual value.
    pub fn validate_preference_items(&self) -> Result<LoginItemState, MenuBarError> {
        self.validate()?;
        let menu = self
            .status_item
            .menu(self.marker)
            .ok_or(MenuBarError::MissingMenu)?;
        for (index, expected, item) in [
            (3, sel!(toggleDensity:), &self.density_item),
            (4, sel!(toggleCompletionSound:), &self.completion_sound_item),
            (5, sel!(toggleLaunchAtLogin:), &self.launch_at_login_item),
        ] {
            if menu.itemAtIndex(index).as_deref() != Some(item)
                || item.action() != Some(expected)
                || item.target().is_none()
            {
                return Err(MenuBarError::PreferenceItems);
            }
        }
        let current_density = density().map_err(|_| MenuBarError::PreferenceItems)?;
        self.set_density_state(current_density);
        let sound = completion_sound_enabled();
        self.set_completion_sound_state(sound);
        let login_item = login_item_status();
        self.set_launch_at_login_state(login_item);
        if self.density_item.state() != item_state(current_density == Density::Standard)
            || self.completion_sound_item.state() != item_state(sound)
            || self.launch_at_login_item.state()
                != item_state(login_item == LoginItemState::Enabled)
        {
            return Err(MenuBarError::PreferenceItems);
        }
        Ok(login_item)
    }

    /// Queues the same `terminate:` action configured on the quit menu item.
    ///
    /// Queuing avoids re-entering GPUI's termination callback while its application state
    /// is borrowed. The native object crosses the queue boundary in `MainThreadBound`, so
    /// it can only be accessed again on the main thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the native startup invariants no longer hold. Successful return
    /// confirms queueing, not the later process termination.
    pub fn perform_quit_action(&self) -> Result<(), MenuBarError> {
        self.validate()?;
        let quit_item = MainThreadBound::new(self.quit_item.clone(), self.marker);
        DispatchQueue::main().exec_async(move || {
            let marker = MainThreadMarker::new()
                .expect("the main dispatch queue must run on the main thread");
            let quit_item = quit_item.into_inner(marker);
            let Some(action) = quit_item.action() else {
                return;
            };
            let application = NSApplication::sharedApplication(marker);
            // SAFETY: `action` is privately fixed to `terminate:` at construction. A nil
            // target uses AppKit's responder chain, exactly as the native menu item does.
            let _ = unsafe { application.sendAction_to_from(action, None, None) };
        });
        Ok(())
    }
}

impl fmt::Debug for MenuBar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MenuBar")
            .field("visible", &self.status_item.isVisible())
            .field("quit_action", &self.quit_item.action())
            .finish_non_exhaustive()
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        self.status_item.setMenu(None);
        self.status_bar.removeStatusItem(&self.status_item);
    }
}

/// Failures while constructing or dispatching the native menu-bar lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MenuBarError {
    /// AppKit objects were requested from a non-main thread.
    NotMainThread,
    /// The process could not adopt the no-Dock Accessory policy.
    ActivationPolicy,
    /// AppKit rejected the encoded status item icon.
    InvalidStatusIcon,
    /// The status item did not provide its standard button.
    MissingButton,
    /// The status item is not visible in the system menu bar.
    StatusItemNotVisible,
    /// The status item's button lost its image.
    MissingStatusImage,
    /// The native status item menu is absent.
    MissingMenu,
    /// The quit menu item does not have the expected action.
    MissingQuitAction,
    /// Native capture commands did not reach their retained target in order.
    CaptureCommands,
    /// The three preference items are missing, mistargeted or show an untrue state.
    PreferenceItems,
    /// The status item menu did not yield the expected five command items.
    MissingPreferenceItems,
}

impl fmt::Display for MenuBarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotMainThread => "menu bar must be created on the main thread",
            Self::ActivationPolicy => "could not activate the no-Dock Accessory policy",
            Self::InvalidStatusIcon => "invalid status item PNG",
            Self::MissingButton => "the status item has no button",
            Self::StatusItemNotVisible => "the status item is not visible",
            Self::MissingStatusImage => "the status item has no image",
            Self::MissingMenu => "the status item has no menu",
            Self::MissingQuitAction => "the quit item has no terminate action",
            Self::CaptureCommands => "capture menu actions could not be delivered",
            Self::PreferenceItems => "preference menu items are missing or display an untrue state",
            Self::MissingPreferenceItems => "the status menu is missing preference items",
        };
        formatter.write_str(message)
    }
}

impl Error for MenuBarError {}
