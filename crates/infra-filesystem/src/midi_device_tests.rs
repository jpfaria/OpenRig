use super::*;

#[test]
fn round_trip_through_yaml_preserves_all_fields() {
    let original = MidiDeviceSelection {
        port_key: MidiPortKey {
            name: "USB MIDI".into(),
            instance: 2,
        },
        alias: "Studio rack".into(),
        enabled: true,
    };
    let yaml = serde_yaml::to_string(&original).unwrap();
    let back: MidiDeviceSelection = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back, original);
}

#[test]
fn missing_instance_field_defaults_to_zero() {
    let yaml = "port_key:\n  name: Foo\nalias: Foo\nenabled: true\n";
    let back: MidiDeviceSelection = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(back.port_key.instance, 0);
}

#[test]
fn missing_enabled_field_defaults_to_false() {
    let yaml = "port_key:\n  name: Foo\nalias: Foo\n";
    let back: MidiDeviceSelection = serde_yaml::from_str(yaml).unwrap();
    assert!(!back.enabled);
}
