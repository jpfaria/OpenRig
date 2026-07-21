//! Enumerate the system's MIDI input ports for the GUI's Settings screen.
//! Pure with respect to midir state: returns a snapshot. The daemon uses
//! the same function so the GUI and the runtime never disagree on which
//! port is `instance = 1`.
//!
//! `MidiPortKey { name, instance }` matches `infra_filesystem::MidiPortKey`
//! by shape; we mirror the type in this crate so `adapter-midi` keeps no
//! infra dependency. Conversion is a one-liner at the call site.

use anyhow::{Context, Result};
use midir::MidiInput;

const CLIENT_NAME: &str = "openrig-enumerate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortKey {
    pub name: String,
    pub instance: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortInfo {
    pub key: MidiPortKey,
    pub raw_name: String,
}

/// Snapshot the available input ports and assign per-name instance counters
/// (0 if unique, otherwise 1..N in discovery order).
pub fn list_input_ports() -> Result<Vec<MidiPortInfo>> {
    let client = MidiInput::new(CLIENT_NAME).context("creating MIDI enumerator")?;
    let raw_names: Vec<String> = client
        .ports()
        .iter()
        .map(|p| client.port_name(p).unwrap_or_default())
        .collect();
    Ok(assign_instances(raw_names))
}

/// Pure: turn an in-order list of raw port names into the disambiguated
/// `MidiPortInfo` list. Extracted so unit tests don't need midir.
pub(crate) fn assign_instances(raw_names: Vec<String>) -> Vec<MidiPortInfo> {
    use std::collections::HashMap;
    let mut counts: HashMap<String, u32> = HashMap::new();
    for name in &raw_names {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }
    let mut seen: HashMap<String, u32> = HashMap::new();
    raw_names
        .into_iter()
        .map(|raw_name| {
            let total = counts[&raw_name];
            let instance = if total == 1 {
                0
            } else {
                let n = seen.entry(raw_name.clone()).or_insert(0);
                *n += 1;
                *n
            };
            MidiPortInfo {
                key: MidiPortKey {
                    name: raw_name.clone(),
                    instance,
                },
                raw_name,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "enumerate_tests.rs"]
mod tests;
