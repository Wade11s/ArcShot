//! CORE-HOTKEY: neutral global-hotkey value, validity and persistence codec.
use arcshot_core::Hotkey;

fn hotkey(key: char, command: bool, control: bool, option: bool, shift: bool) -> Hotkey {
    Hotkey::new(key, command, control, option, shift)
}

#[test]
fn core_hotkey_01_default_is_command_shift_x() {
    assert_eq!(Hotkey::default(), hotkey('x', true, false, false, true));
    assert!(Hotkey::default().is_valid());
    assert_eq!(Hotkey::default().to_string(), "⌘⇧X");
}

#[test]
fn core_hotkey_02_validity_requires_one_qualifying_modifier_and_a_plain_key() {
    // Command / Control / Option each qualify; Shift may be added.
    assert!(hotkey('x', true, false, false, false).is_valid());
    assert!(hotkey('x', false, true, false, false).is_valid());
    assert!(hotkey('x', false, false, true, false).is_valid());
    assert!(hotkey('x', true, true, true, true).is_valid());
    assert!(hotkey('5', false, true, false, true).is_valid());
    assert!(hotkey(',', false, false, true, false).is_valid());
    // Shift plus a plain key alone is not a legal global shortcut.
    assert!(!hotkey('x', false, false, false, true).is_valid());
    // No modifier at all is not legal either.
    assert!(!hotkey('x', false, false, false, false).is_valid());
    // A control character is never a plain key.
    assert!(!hotkey('\n', true, false, false, false).is_valid());
}

#[test]
fn core_hotkey_03_display_lists_modifiers_in_fixed_order() {
    assert_eq!(hotkey('q', true, true, false, true).to_string(), "⌘⌃⇧Q");
    assert_eq!(hotkey('f', false, false, true, false).to_string(), "⌥F");
    assert_eq!(hotkey(' ', true, false, false, false).to_string(), "⌘空格");
}

#[test]
fn core_hotkey_04_storage_round_trips_and_rejects_malformed_values() {
    let original = hotkey('x', true, false, false, true);
    let stored = original.to_storage_string();
    assert_eq!(stored, "command+shift+x");
    assert_eq!(Hotkey::from_storage_string(&stored), Some(original));

    let spaced = hotkey(' ', false, false, true, true);
    assert_eq!(spaced.to_storage_string(), "option+shift+space");
    assert_eq!(
        Hotkey::from_storage_string("option+shift+space"),
        Some(spaced)
    );

    // Illegal combinations, unknown tokens and broken shapes never load.
    for malformed in [
        "",
        "x",
        "shift+x",
        "command+",
        "+x",
        "command+xx",
        "hyper+x",
        "command+command+x",
        "command+shift+",
    ] {
        assert_eq!(
            Hotkey::from_storage_string(malformed),
            None,
            "{malformed:?}"
        );
    }
}
