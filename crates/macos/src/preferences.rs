//! Local per-user preferences for the status-item menu.
//!
//! Values live in the app's own preferences domain; nothing is synced, uploaded
//! or shared between users.

use std::{error::Error, fmt};

use arcshot_core::Density;
use objc2_foundation::{NSString, NSUserDefaults};

/// Preferences key for 完成音效; an absent key means the documented default.
const COMPLETION_SOUND_KEY: &str = "CompletionSoundEnabled";
/// Preferences key for 选项密度; a stored string token, absent means Compact.
const DENSITY_KEY: &str = "OptionDensity";

/// Reads the persisted 完成音效 preference from the app's preferences domain.
///
/// The documented default is on, so a first launch (no stored key) plays the sound.
#[must_use]
pub fn completion_sound_enabled() -> bool {
    read_completion_sound(&NSUserDefaults::standardUserDefaults())
}

/// Persists the 完成音效 choice locally so later launches restore it.
pub fn store_completion_sound(enabled: bool) {
    write_completion_sound(&NSUserDefaults::standardUserDefaults(), enabled);
}

fn read_completion_sound(defaults: &NSUserDefaults) -> bool {
    let key = NSString::from_str(COMPLETION_SOUND_KEY);
    // `boolForKey` cannot distinguish "absent" from "off"; absence is the on default.
    defaults.objectForKey(&key).is_none() || defaults.boolForKey(&key)
}

fn write_completion_sound(defaults: &NSUserDefaults, enabled: bool) {
    defaults.setBool_forKey(enabled, &NSString::from_str(COMPLETION_SOUND_KEY));
    // Flush to disk now: the user may quit immediately after the menu click and
    // the next launch must still see this choice.
    defaults.synchronize();
}

/// Reads the persisted 选项密度 from the app's own preferences domain.
///
/// An absent key is the documented Compact default; a present but malformed
/// value is a read failure, reported instead of silently changing the intent.
///
/// # Errors
///
/// Returns [`DensityError::Unreadable`] when the stored object is not a valid
/// `Standard` / `Compact` token.
pub fn density() -> Result<Density, DensityError> {
    read_density(&NSUserDefaults::standardUserDefaults())
}

/// Persists the 选项密度 choice locally so later launches restore it.
///
/// # Errors
///
/// Returns [`DensityError::NotPersisted`] when the adapter cannot flush the
/// write, so the caller never reports an unconfirmed intent as applied.
pub fn store_density(density: Density) -> Result<(), DensityError> {
    write_density(&NSUserDefaults::standardUserDefaults(), density)
}

fn read_density(defaults: &NSUserDefaults) -> Result<Density, DensityError> {
    let key = NSString::from_str(DENSITY_KEY);
    if defaults.objectForKey(&key).is_none() {
        return Ok(Density::default());
    }
    let stored = defaults
        .stringForKey(&key)
        .ok_or(DensityError::Unreadable)?;
    Density::from_stored(&stored.to_string()).ok_or(DensityError::Unreadable)
}

fn write_density(defaults: &NSUserDefaults, density: Density) -> Result<(), DensityError> {
    let key = NSString::from_str(DENSITY_KEY);
    // SAFETY: `NSString` is a valid property-list object for NSUserDefaults;
    // the temporary key and value both outlive this call.
    unsafe {
        defaults.setObject_forKey(Some(&NSString::from_str(density.as_str())), &key);
    }
    // Flush now: the user may quit immediately after the menu click.
    if defaults.synchronize() {
        Ok(())
    } else {
        Err(DensityError::NotPersisted)
    }
}

/// A density value could not be read or could not be confirmed persisted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DensityError {
    /// The stored value is present but is not a Standard / Compact token.
    Unreadable,
    /// The write could not be flushed to disk.
    NotPersisted,
}

impl fmt::Display for DensityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unreadable => "stored option density is unreadable",
            Self::NotPersisted => "option density could not be persisted",
        })
    }
}

impl Error for DensityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::{AnyThread, rc::Retained}; // alloc() needs the AnyThread trait

    /// An isolated, real preferences domain so the user's own choice is never touched.
    fn suite(name: &str) -> Retained<NSUserDefaults> {
        NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&NSString::from_str(name)))
            .expect("suite defaults")
    }

    #[test]
    fn mac_start_30_02_completion_sound_round_trips_in_a_real_preferences_domain() {
        let name = format!("dev.arcshot.preferences-test-{}", std::process::id());
        let defaults = suite(&name);
        let key = NSString::from_str(COMPLETION_SOUND_KEY);
        defaults.removeObjectForKey(&key);
        assert!(
            read_completion_sound(&defaults),
            "an absent key must read as the documented on default"
        );
        write_completion_sound(&defaults, false);
        assert!(!read_completion_sound(&defaults));
        assert!(
            !read_completion_sound(&suite(&name)),
            "a separate defaults object reads the same domain"
        );
        write_completion_sound(&defaults, true);
        assert!(read_completion_sound(&defaults));
        defaults.removeObjectForKey(&key);
        assert!(
            read_completion_sound(&defaults),
            "removal restores the default"
        );
        // Leaving the isolated domain behind would be a user-visible artifact.
        defaults.removePersistentDomainForName(&NSString::from_str(&name));
    }

    #[test]
    fn mac_start_29_02_density_round_trips_in_a_real_preferences_domain() {
        let name = format!("dev.arcshot.density-test-{}", std::process::id());
        let defaults = suite(&name);
        let key = NSString::from_str(DENSITY_KEY);
        defaults.removeObjectForKey(&key);
        assert_eq!(
            read_density(&defaults),
            Ok(Density::Compact),
            "an absent key reads as the documented Compact default"
        );

        write_density(&defaults, Density::Standard).unwrap();
        assert_eq!(read_density(&defaults), Ok(Density::Standard));
        assert_eq!(
            read_density(&suite(&name)),
            Ok(Density::Standard),
            "a separate defaults object reads the same domain"
        );
        write_density(&defaults, Density::Compact).unwrap();
        assert_eq!(read_density(&defaults), Ok(Density::Compact));

        // A present but malformed token is a reported read failure, not a
        // silent fallback that would change the user's intent.
        // SAFETY: NSString is a valid plist object; key/value outlive the call.
        unsafe {
            defaults.setObject_forKey(Some(&NSString::from_str("Bogus")), &key);
        }
        assert_eq!(read_density(&defaults), Err(DensityError::Unreadable));

        defaults.removeObjectForKey(&key);
        assert_eq!(read_density(&defaults), Ok(Density::Compact));
        defaults.removePersistentDomainForName(&NSString::from_str(&name));
    }
}
