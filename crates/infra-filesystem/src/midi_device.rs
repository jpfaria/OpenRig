//! Per-machine MIDI device selection persisted to `config.yaml`. Identity is
//! `MidiPortKey { name, instance }` so two physically distinct devices with
//! the same OS-reported name remain addressable; the user-editable `alias`
//! makes them visually unambiguous in the GUI.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default, JsonSchema)]
pub struct MidiPortKey {
    pub name: String,
    /// Disambiguator for ports sharing the same `name`. Assigned in
    /// enumeration order at first detection. `0` means "the only port with
    /// this name on this machine".
    #[serde(default)]
    pub instance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub struct MidiDeviceSelection {
    pub port_key: MidiPortKey,
    /// User-facing label. Defaults to the raw OS port name on first
    /// detection; editable from the GUI.
    pub alias: String,
    #[serde(default)]
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_yaml_preserves_all_fields() {
        let original = MidiDeviceSelection {
            port_key: MidiPortKey { name: "USB MIDI".into(), instance: 2 },
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
}
