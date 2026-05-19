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

/// Indices of **every** input port to open: all whose name contains
/// `wanted` (case-insensitive) when set, else **all** ports. Returning
/// every match is what lets several identical controllers (e.g. 4
/// Chocolates) run at once. Pure so the rule is unit-tested without a
/// device.
fn select_ports(available: &[String], wanted: Option<&str>) -> Vec<usize> {
    match wanted {
        Some(w) => {
            let w = w.to_lowercase();
            available
                .iter()
                .enumerate()
                .filter(|(_, n)| n.to_lowercase().contains(&w))
                .map(|(i, _)| i)
                .collect()
        }
        None => (0..available.len()).collect(),
    }
}

/// Load the map at `map_path`, open **every** matching MIDI input, and
/// run until the process exits. Call from a dedicated thread. midir
/// consumes one `MidiInput` per connection, so we create one per port;
/// all callbacks submit to the **same** command bridge (clone is
/// cheap + `Send`). Submitting is fire-and-forget: a footswitch does
/// not block on the dispatch result.
pub fn run_blocking(bridge: CommandBridge, map_path: &Path) -> Result<()> {
    let map = std::sync::Arc::new(MidiMap::load(map_path)?);

    // One throwaway client just to enumerate ports + names.
    let enumerator = MidiInput::new(CLIENT_NAME).context("creating MIDI input client")?;
    let names: Vec<String> = enumerator
        .ports()
        .iter()
        .map(|p| enumerator.port_name(p).unwrap_or_default())
        .collect();
    drop(enumerator);

    let selected = select_ports(&names, map.input.as_deref());
    if selected.is_empty() {
        return Err(anyhow!(
            "no MIDI input port matched {:?} (available: {:?})",
            map.input,
            names
        ));
    }

    // Each connection owns its callback thread; keep them all bound
    // until the process exits (dropping a connection closes its port).
    let mut connections = Vec::with_capacity(selected.len());
    for idx in selected {
        let client = MidiInput::new(CLIENT_NAME).context("creating MIDI input client")?;
        // `ports()` is stable across these short-lived clients (same
        // backend snapshot); re-fetch so this client owns the handle.
        let ports = client.ports();
        let Some(port) = ports.get(idx) else {
            continue;
        };
        let name = names[idx].clone();
        let map = std::sync::Arc::clone(&map);
        let bridge = bridge.clone();
        let conn = client
            .connect(
                port,
                PORT_NAME,
                move |_stamp, bytes, _| {
                    if let Some(msg) = MidiMessage::parse(bytes) {
                        if let Some(cmd) = resolve(&map, &msg) {
                            // Fire-and-forget (footswitch does not await).
                            drop(bridge.submit(cmd));
                        }
                    }
                },
                (),
            )
            .map_err(|e| anyhow!("connecting to MIDI port '{name}': {e}"))?;
        log::info!("adapter-midi: listening on '{name}'");
        connections.push(conn);
    }

    if connections.is_empty() {
        return Err(anyhow!("no MIDI input port could be opened"));
    }

    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_ports_returns_all_matching_so_several_pedals_work() {
        let ports = vec![
            "IAC Driver Bus 1".to_string(),
            "M-VAVE Chocolate".to_string(),
            "Chocolate Plus".to_string(),
        ];
        assert_eq!(select_ports(&ports, Some("chocolate")), vec![1, 2]);
    }

    #[test]
    fn select_ports_none_opens_every_port() {
        let ports = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        assert_eq!(select_ports(&ports, None), vec![0, 1, 2]);
    }

    #[test]
    fn select_ports_empty_when_no_match() {
        let ports = vec!["A".to_string()];
        assert!(select_ports(&ports, Some("nope")).is_empty());
    }

    #[test]
    fn select_ports_empty_when_no_ports() {
        assert!(select_ports(&[], None).is_empty());
    }
}
