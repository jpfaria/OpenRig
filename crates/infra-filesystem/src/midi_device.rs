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
#[path = "midi_device_tests.rs"]
mod tests;
