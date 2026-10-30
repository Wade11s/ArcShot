//! The neutral global capture hotkey shared by settings, persistence and native adapters.

use std::fmt;

/// A global capture hotkey: modifier flags plus one plain key character.
///
/// The struct itself holds whatever the recorder produced; validity is a
/// separate predicate because the settings UI must be able to display an
/// illegal candidate before rejecting it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Hotkey {
    key: char,
    command: bool,
    control: bool,
    option: bool,
    shift: bool,
}

impl Hotkey {
    /// Records a combination exactly as pressed; does not validate.
    #[must_use]
    pub const fn new(key: char, command: bool, control: bool, option: bool, shift: bool) -> Self {
        Self {
            key,
            command,
            control,
            option,
            shift,
        }
    }

    /// The plain key character, as typed (letters are stored lowercase).
    #[must_use]
    pub const fn key(self) -> char {
        self.key
    }

    #[must_use]
    pub const fn command(self) -> bool {
        self.command
    }

    #[must_use]
    pub const fn control(self) -> bool {
        self.control
    }

    #[must_use]
    pub const fn option(self) -> bool {
        self.option
    }

    #[must_use]
    pub const fn shift(self) -> bool {
        self.shift
    }

    /// A legal combination contains at least one Command / Control / Option
    /// modifier and one plain key; Shift may be added. Shift plus a plain key
    /// alone, or no modifier at all, is not legal.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        (self.command || self.control || self.option) && !self.key.is_control()
    }

    /// The canonical persisted representation, e.g. `command+shift+x`.
    #[must_use]
    pub fn to_storage_string(self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.command {
            parts.push("command".to_owned());
        }
        if self.control {
            parts.push("control".to_owned());
        }
        if self.option {
            parts.push("option".to_owned());
        }
        if self.shift {
            parts.push("shift".to_owned());
        }
        parts.push(match self.key {
            ' ' => "space".to_owned(),
            key => key.to_lowercase().next().unwrap_or(key).to_string(),
        });
        parts.join("+")
    }

    /// Parses the canonical storage representation; illegal combinations and
    /// malformed shapes return `None` so callers fall back to the default.
    #[must_use]
    pub fn from_storage_string(value: &str) -> Option<Self> {
        let mut command = false;
        let mut control = false;
        let mut option = false;
        let mut shift = false;
        let mut key: Option<char> = None;
        for token in value.split('+') {
            match token {
                "command" if !command => command = true,
                "control" if !control => control = true,
                "option" if !option => option = true,
                "shift" if !shift => shift = true,
                "space" if key.is_none() => key = Some(' '),
                _ => {
                    let mut chars = token.chars();
                    let (parsed, single) = (chars.next(), chars.next().is_none());
                    key = match (parsed, single, key.is_none()) {
                        (Some(parsed), true, true) => Some(parsed.to_lowercase().next()?),
                        _ => return None,
                    };
                }
            }
        }
        let key = key?;
        let hotkey = Self::new(key, command, control, option, shift);
        hotkey.is_valid().then_some(hotkey)
    }
}

impl Default for Hotkey {
    /// The product default: Command + Shift + X.
    fn default() -> Self {
        Self::new('x', true, false, false, true)
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.command {
            formatter.write_str("⌘")?;
        }
        if self.control {
            formatter.write_str("⌃")?;
        }
        if self.option {
            formatter.write_str("⌥")?;
        }
        if self.shift {
            formatter.write_str("⇧")?;
        }
        match self.key {
            ' ' => formatter.write_str("空格"),
            key => write!(formatter, "{}", key.to_uppercase()),
        }
    }
}
