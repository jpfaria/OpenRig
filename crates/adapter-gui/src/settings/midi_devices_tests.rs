//! Wiring tests for the System / MIDI devices section (#513). No AppWindow
//! is constructed — the tests drive the pure wiring functions and assert
//! on the captured Command stream.

use super::{devices_for_save, edit_alias, merge_enumeration, toggle_row};
use infra_filesystem::{MidiDeviceSelection, MidiPortKey};

#[test]
fn merge_seeds_new_rows_with_alias_equal_to_name() {
    let persisted = vec![];
    let enumerated = vec![("USB MIDI".to_string(), 0)];
    let merged = merge_enumeration(persisted, enumerated);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].alias, "USB MIDI");
    assert!(!merged[0].enabled, "newly seen devices default to disabled");
}

#[test]
fn merge_seeds_duplicate_names_with_hash_suffix() {
    let merged = merge_enumeration(vec![], vec![("USB MIDI".into(), 1), ("USB MIDI".into(), 2)]);
    assert_eq!(merged[0].alias, "USB MIDI (#1)");
    assert_eq!(merged[1].alias, "USB MIDI (#2)");
}

#[test]
fn merge_preserves_existing_alias_and_enabled_for_known_keys() {
    let persisted = vec![MidiDeviceSelection {
        port_key: MidiPortKey {
            name: "Foo".into(),
            instance: 0,
        },
        alias: "My Pedal".into(),
        enabled: true,
    }];
    let merged = merge_enumeration(persisted, vec![("Foo".into(), 0)]);
    assert_eq!(merged[0].alias, "My Pedal");
    assert!(merged[0].enabled);
}

#[test]
fn merge_keeps_disappeared_devices_in_the_list_as_disabled() {
    let persisted = vec![MidiDeviceSelection {
        port_key: MidiPortKey {
            name: "Gone".into(),
            instance: 0,
        },
        alias: "Studio Pedal".into(),
        enabled: true,
    }];
    let merged = merge_enumeration(persisted, vec![]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].alias, "Studio Pedal");
    assert!(!merged[0].enabled, "absent device is force-disabled");
}

#[test]
fn toggle_row_flips_enabled_for_matching_key() {
    let mut rows = vec![MidiDeviceSelection {
        port_key: MidiPortKey {
            name: "Foo".into(),
            instance: 0,
        },
        alias: "Foo".into(),
        enabled: false,
    }];
    toggle_row(
        &mut rows,
        &MidiPortKey {
            name: "Foo".into(),
            instance: 0,
        },
        true,
    );
    assert!(rows[0].enabled);
}

#[test]
fn edit_alias_writes_through() {
    let mut rows = vec![MidiDeviceSelection {
        port_key: MidiPortKey {
            name: "Foo".into(),
            instance: 0,
        },
        alias: "Foo".into(),
        enabled: false,
    }];
    edit_alias(
        &mut rows,
        &MidiPortKey {
            name: "Foo".into(),
            instance: 0,
        },
        "New Name",
    );
    assert_eq!(rows[0].alias, "New Name");
}

#[test]
fn devices_for_save_returns_owned_copy() {
    let rows = vec![MidiDeviceSelection {
        port_key: MidiPortKey {
            name: "Foo".into(),
            instance: 0,
        },
        alias: "Foo".into(),
        enabled: true,
    }];
    let copy = devices_for_save(&rows);
    assert_eq!(copy.len(), 1);
}
