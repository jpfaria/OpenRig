//! Red-first (issue #548 hot-plug): the daemon must re-enumerate MIDI
//! ports periodically and attach new ones — devices paired AFTER the
//! app starts (BLE-MIDI, USB plug-in) need to "just work" without an
//! app restart. The actual attach loop is hard to unit-test without a
//! real device, so we pin the public surface: a pure
//! `new_port_names(prev, current)` returns the diff the daemon uses
//! to decide which ports to open this tick.

use adapter_midi::daemon::new_port_names;

#[test]
fn new_port_names_returns_ports_added_since_last_tick() {
    let prev = vec!["IAC".to_string(), "Chocolate".to_string()];
    let current = vec![
        "IAC".to_string(),
        "Chocolate".to_string(),
        "FootCtrlPlus Bluetooth".to_string(),
    ];
    let added = new_port_names(&prev, &current);
    assert_eq!(added, vec!["FootCtrlPlus Bluetooth".to_string()]);
}

#[test]
fn new_port_names_empty_when_nothing_changed() {
    let same = vec!["A".to_string(), "B".to_string()];
    assert!(new_port_names(&same, &same).is_empty());
}

#[test]
fn new_port_names_ignores_ports_that_disappeared() {
    // V1: we only ATTACH new ports; disconnections are not pruned (the
    // existing `MidiInputConnection` errors out on its own when the
    // device vanishes). The function should not return removed names.
    let prev = vec!["A".to_string(), "B".to_string()];
    let current = vec!["A".to_string()];
    assert!(new_port_names(&prev, &current).is_empty());
}
