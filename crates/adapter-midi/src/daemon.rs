//! The only impure layer: open a `midir` input (USB or BLE-MIDI — a paired
//! M-Vave Chocolate shows up here like any other input), parse each message,
//! resolve it through the map, and submit the `Command` over the bridge. The
//! frontend drains and dispatches on its own thread, so this never touches
//! the audio thread.

use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Context, Result};
use application::bridge::CommandBridge;
use application::command::Command;
use application::SelectionState;
use midir::MidiInput;

use crate::learn::LearnState;
use crate::mapping::MidiMap;
use crate::message::MidiMessage;
use crate::pipeline::dispatch_midi_message_to_bridge;
use crate::profile::MidiProfile;
use crate::slots::IncomingMessage;
use crate::translate::{resolve, source_from_bytes};

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

/// Load the legacy single-file map at `map_path`, validate it, and run the
/// daemon. Thin wrapper around [`run_blocking_with_map`] preserved for the
/// `--midi=PATH` (explicit legacy file) flow.
pub fn run_blocking(bridge: CommandBridge, map_path: &Path, learn: Arc<LearnState>) -> Result<()> {
    let map = MidiMap::load(map_path)?;
    run_blocking_with_map(bridge, map, learn)
}

/// Open **every** matching MIDI input for the pre-resolved [`MidiMap`] and
/// run until the process exits. Call from a dedicated thread. midir
/// consumes one `MidiInput` per connection, so we create one per port;
/// all callbacks submit to the **same** command bridge (clone is cheap +
/// `Send`). Submitting is fire-and-forget: a footswitch does not block on
/// the dispatch result.
///
/// `learn` is the process-wide single-shot learn-mode flag (#513 / #493).
/// While `learn.is_active()`, each parseable incoming event is submitted as
/// `Command::PublishMidiEvent { source }` and the flag auto-clears via
/// `learn.on_event_captured()`. The binding-resolution path is **skipped**
/// while learning so the user does not accidentally fire whatever the
/// pedal is already mapped to. When the flag is off, behaviour is
/// unchanged: every event resolves through the binding map.
pub fn run_blocking_with_map(
    bridge: CommandBridge,
    map: MidiMap,
    learn: Arc<LearnState>,
) -> Result<()> {
    let map = std::sync::Arc::new(map);

    let infos = crate::enumerate::list_input_ports()?;
    let names: Vec<String> = infos.iter().map(|i| i.raw_name.clone()).collect();

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
        let learn = Arc::clone(&learn);
        let conn = client
            .connect(
                port,
                PORT_NAME,
                move |_stamp, bytes, _| {
                    // #513 / #493: learn-mode short-circuits the binding
                    // path. Lock-free atomic — same hot-path invariants
                    // as the rest of the callback (no mutex, no alloc,
                    // no syscall). When the flag is off the load is a
                    // single relaxed-ish read with no side effects.
                    if learn.is_active() {
                        if let Some(source) = source_from_bytes(bytes) {
                            drop(bridge.submit(Command::PublishMidiEvent { source }));
                            learn.on_event_captured();
                            return;
                        }
                        // Unparseable bytes (system real-time, truncated,
                        // unsupported voice messages) do NOT auto-disarm
                        // the flag — the user is still waiting for the
                        // pedal press they actually meant to learn.
                    }
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

/// New profile-driven path (issue #548). Opens **every** MIDI input
/// port, parses each incoming byte stream into an [`IncomingMessage`],
/// and routes through every active profile's bindings into the
/// matching `Command`. The legacy [`run_blocking_with_map`] is preserved
/// for the `--midi=PATH` (single-file map) flow; the GUI's standard
/// path calls `run_blocking_with_profiles` once profiles land in the
/// project's MIDI config.
///
/// `selection` is the live GUI snapshot the slots read (active
/// chain/block, toggle flags, …) — shared with the dispatcher via
/// `LocalDispatcher::selection_state()` so the GUI's writes are
/// immediately visible here. `learn` honours the same short-circuit
/// rule the legacy path uses.
pub fn run_blocking_with_profiles(
    bridge: CommandBridge,
    profiles: Vec<MidiProfile>,
    selection: Arc<RwLock<SelectionState>>,
    learn: Arc<LearnState>,
) -> Result<()> {
    let profiles = Arc::new(profiles);
    let infos = crate::enumerate::list_input_ports()?;
    if infos.is_empty() {
        return Err(anyhow!("no MIDI input port available"));
    }

    let mut connections = Vec::with_capacity(infos.len());
    for (idx, info) in infos.iter().enumerate() {
        let client = MidiInput::new(CLIENT_NAME).context("creating MIDI input client")?;
        let ports = client.ports();
        let Some(port) = ports.get(idx) else {
            continue;
        };
        let port_name = info.raw_name.clone();
        let profiles = Arc::clone(&profiles);
        let selection = Arc::clone(&selection);
        let bridge = bridge.clone();
        let learn = Arc::clone(&learn);
        let conn = client
            .connect(
                port,
                PORT_NAME,
                move |_stamp, bytes, _| {
                    if learn.is_active() {
                        if let Some(source) = source_from_bytes(bytes) {
                            drop(bridge.submit(Command::PublishMidiEvent { source }));
                            learn.on_event_captured();
                            return;
                        }
                        return;
                    }
                    let Some(msg) = IncomingMessage::from_bytes(bytes) else {
                        return;
                    };
                    // Snapshot SelectionState under a short read lock —
                    // the lock is released before bridge.submit() so a
                    // concurrent GUI writer never blocks on us.
                    let snapshot = match selection.read() {
                        Ok(g) => g.clone(),
                        Err(_) => return, // poisoned → drop this event
                    };
                    let active: Vec<&MidiProfile> = profiles.iter().collect();
                    dispatch_midi_message_to_bridge(
                        &active,
                        &port_name,
                        &msg,
                        &snapshot,
                        &bridge,
                    );
                },
                (),
            )
            .map_err(|e| anyhow!("connecting to MIDI port '{}': {e}", info.raw_name))?;
        log::info!("adapter-midi: listening on '{}' (profiles)", info.raw_name);
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
