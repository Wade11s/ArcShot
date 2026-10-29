//! Safe macOS adapters for ArcShot.

mod permission;

pub use permission::{
    PermissionError, mark_permission_guide_seen, open_screen_recording_settings,
    permission_guide_seen, request_screen_recording, screen_recording_available,
};

use std::{error::Error, fmt};

use arcshot_core::{MenuBarText, StatusItemIcon};
use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, rc::Retained, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuItem,
    NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use objc2_foundation::{NSData, NSSize, NSString};

/// Standard status-item point size; the 36-pixel asset renders at 2x without scaling.
const STATUS_ITEM_SIDE_POINTS: f64 = 18.0;

/// A native menu-bar status item whose resources are owned on the main thread.
pub struct MenuBar {
    marker: MainThreadMarker,
    status_bar: Retained<NSStatusBar>,
    status_item: Retained<NSStatusItem>,
    quit_item: Retained<NSMenuItem>,
}

impl MenuBar {
    /// Creates the status item and its single implemented command, Quit ArcShot.
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
            quit_item,
        };
        menu_bar.validate()?;
        Ok(menu_bar)
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
        };
        formatter.write_str(message)
    }
}

impl Error for MenuBarError {}
