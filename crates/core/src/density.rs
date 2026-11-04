//! I-09 选项密度: a pure, session-scoped display preference.
//!
//! Density is a preference, not a [`crate::Document`] edit: it lives on the
//! capture session so it survives new captures and restarts, and changing it can
//! never disturb the authoritative tool, styles, selection or objects. Both
//! variants expose exactly the same controls; only the toolbar layout differs.

/// Standard / Compact option density; the documented default is Compact.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Density {
    /// Group headings and larger icons, dock and option strips.
    Standard,
    /// Compressed titles, icons and spacing.
    #[default]
    Compact,
}

impl Density {
    /// Stable token stored in the app's own preferences domain.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Compact => "Compact",
        }
    }

    /// Decodes a persisted token. An unknown token is a read failure, not a
    /// silent fallback to the default: the caller reports it instead of
    /// changing the user's intent.
    #[must_use]
    pub fn from_stored(token: &str) -> Option<Self> {
        match token {
            "Standard" => Some(Self::Standard),
            "Compact" => Some(Self::Compact),
            _ => None,
        }
    }

    /// The other density; one menu command switches between the two.
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Standard => Self::Compact,
            Self::Compact => Self::Standard,
        }
    }
}
