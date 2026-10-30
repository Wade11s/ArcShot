//! Platform-neutral data shared by ArcShot's product crates.

mod frame;
mod hotkey;
mod session;

pub use frame::{CapturedFrame, FrameError, ScreenGeometry};
pub use hotkey::Hotkey;
pub use session::{
    CaptureAction, CaptureEffect, CaptureFailure, CapturePhase, CaptureSession, CaptureTicket,
};

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
