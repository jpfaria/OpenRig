//! The only impure layer: open a `midir` input (USB or BLE-MIDI — a paired
//! M-Vave Chocolate shows up here like any other input), parse each message,
//! resolve it through the map, and submit the `Command` over the bridge. The
//! frontend drains and dispatches on its own thread, so this never touches
//! the audio thread.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use application::bridge::CommandBridge;
use midir::MidiInput;

use crate::mapping::MidiMap;
use crate::message::MidiMessage;
use crate::translate::resolve;

const CLIENT_NAME: &str = "OpenRig";
const PORT_NAME: &str = "openrig-midi-in";

/// Pick an input port: the first whose name contains `wanted`
/// (case-insensitive) when set, else the first available port. Pure so the
/// selection rule is unit-tested without a device.
fn select_port(available: &[String], wanted: Option<&str>) -> Option<usize> {
    match wanted {
        Some(w) => {
            let w = w.to_lowercase();
            available.iter().position(|n| n.to_lowercase().contains(&w))
        }
        None => (!available.is_empty()).then_some(0),
    }
}

/// Load the map at `map_path`, open the selected MIDI input, and run until
/// the process exits. Call from a dedicated thread. Submitting is
/// fire-and-forget: a footswitch does not block on the dispatch result, so
/// the bridge's reply receiver is dropped.
pub fn run_blocking(bridge: CommandBridge, map_path: &Path) -> Result<()> {
    let map = MidiMap::load(map_path)?;

    let midi_in = MidiInput::new(CLIENT_NAME).context("creating MIDI input client")?;
    let ports = midi_in.ports();
    let names: Vec<String> = ports
        .iter()
        .map(|p| midi_in.port_name(p).unwrap_or_default())
        .collect();

    let idx = select_port(&names, map.input.as_deref()).ok_or_else(|| {
        anyhow!(
            "no MIDI input port matched {:?} (available: {:?})",
            map.input,
            names
        )
    })?;
    log::info!("adapter-midi: listening on '{}'", names[idx]);

    // The connection owns the callback thread; keep it bound until exit.
    let _conn = midi_in
        .connect(
            &ports[idx],
            PORT_NAME,
            move |_stamp, bytes, _| {
                if let Some(msg) = MidiMessage::parse(bytes) {
                    if let Some(cmd) = resolve(&map, &msg) {
                        // Fire-and-forget: `submit` already enqueued the
                        // command synchronously; the returned reply future
                        // is intentionally discarded (a footswitch does not
                        // await the dispatch result).
                        drop(bridge.submit(cmd));
                    }
                }
            },
            (),
        )
        .map_err(|e| anyhow!("connecting to MIDI port '{}': {e}", names[idx]))?;

    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_port_substring_case_insensitive() {
        let ports = vec![
            "IAC Driver Bus 1".to_string(),
            "M-VAVE Chocolate".to_string(),
        ];
        assert_eq!(select_port(&ports, Some("chocolate")), Some(1));
    }

    #[test]
    fn select_port_defaults_to_first_when_unset() {
        let ports = vec!["A".to_string(), "B".to_string()];
        assert_eq!(select_port(&ports, None), Some(0));
    }

    #[test]
    fn select_port_none_when_no_match() {
        let ports = vec!["A".to_string()];
        assert_eq!(select_port(&ports, Some("nope")), None);
    }

    #[test]
    fn select_port_none_when_no_ports() {
        assert_eq!(select_port(&[], None), None);
    }
}
