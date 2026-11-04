//! Local per-user preferences for the status-item menu.
//!
//! Values live in the app's own preferences domain; nothing is synced, uploaded
//! or shared between users.

use objc2_foundation::{NSString, NSUserDefaults};

/// Preferences key for 完成音效; an absent key means the documented default.
const COMPLETION_SOUND_KEY: &str = "CompletionSoundEnabled";

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

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::{AnyThread, rc::Retained};

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
}
