//! Platform-neutral data shared by ArcShot's product crates.

mod annotation;
mod density;
mod document;
mod frame;
mod hotkey;
mod session;

pub use annotation::{
    Annotation, PaletteColor, RectangleOption, RectangleStyle, StrokeWidth, Tool, annotation_at,
};
pub use density::Density;
pub use document::{
    Document, DocumentError, DragTarget, MAX_PNG_BYTES, PixelRect, ResizeHandle, Selection,
};
pub use frame::{CapturedFrame, FrameError, MAX_EDGE, MAX_PIXELS, ScreenGeometry};
pub use hotkey::Hotkey;
pub use session::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
    OutputFailure, OutputJob, OutputTarget, SavePanelResult,
};

/// Actual login-item state reported by the system, never a requested value.
///
/// Menu presentation and policy both derive from a fresh read of this value, so
/// an unconfirmed request can never be displayed as an enabled login item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoginItemState {
    /// Registered and eligible to launch the app at login.
    Enabled,
    /// Not registered, so the app does not launch at login.
    Disabled,
    /// Registered but waiting for the user to approve it in System Settings.
    RequiresApproval,
}

/// Text displayed by the native menu-bar interface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MenuBarText {
    tooltip: &'static str,
    quit_item: &'static str,
}

impl MenuBarText {
    /// Creates the standard ArcShot menu-bar text.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tooltip: "ArcShot",
            quit_item: "退出 ArcShot",
        }
    }

    /// Returns the status item's accessibility tooltip.
    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        self.tooltip
    }

    /// Returns the visible title of the quit menu item.
    #[must_use]
    pub const fn quit_item(self) -> &'static str {
        self.quit_item
    }
}

impl Default for MenuBarText {
    fn default() -> Self {
        Self::new()
    }
}

/// An encoded PNG used by the native menu-bar status item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StatusItemIcon {
    png_bytes: &'static [u8],
}

impl StatusItemIcon {
    /// Wraps PNG bytes without interpreting them on the neutral-data layer.
    #[must_use]
    pub const fn from_png(png_bytes: &'static [u8]) -> Self {
        Self { png_bytes }
    }

    /// Returns the encoded PNG bytes.
    #[must_use]
    pub const fn png_bytes(self) -> &'static [u8] {
        self.png_bytes
    }
}
