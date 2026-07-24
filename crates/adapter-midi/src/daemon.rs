//! The only impure layer: open a `midir` input (USB or BLE-MIDI — a paired
//! M-Vave Chocolate shows up here like any other input), parse each message,
//! resolve it through the map, and submit the `Command` over the bridge. The
//! frontend drains and dispatches on its own thread, so this never touches
//! the audio thread.

use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Context, Result};
use application::bridge::CommandBridge;
use application::command::{Command, MidiCommand};
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

/// Port names present in `current` that were not in `prev`. Drives the
/// daemon's hot-plug loop: each tick we re-enumerate, ask this for the
/// delta, and attach a connection for every new port. Disconnections
/// are NOT pruned — midir's `MidiInputConnection` errors out on its
/// own when the device vanishes, which is enough for V1.
pub fn new_port_names(prev: &[String], current: &[String]) -> Vec<String> {
    current
        .iter()
        .filter(|name| !prev.iter().any(|p| p == *name))
        .cloned()
        .collect()
}

/// Force a CoreMIDI restart + re-enumerate up to `retries` times with
/// `gap` between attempts, stopping as soon as the port set grows
/// beyond `prev`. The union of every snapshot we saw is returned, so a
/// port that briefly flickered in then out still counts as "seen".
///
/// CoreMIDI surfaces BLE-MIDI ports asynchronously after a restart;
/// a single restart + sleep + enumerate is empirically not enough.
/// On non-macOS targets the restart calls are skipped — the loop is
/// still useful (some USB stacks are also slow to publish).
pub fn scan_with_retry(prev: &[String], retries: usize, gap: std::time::Duration) -> Vec<String> {
    use std::collections::HashSet;
    let mut union: HashSet<String> = prev.iter().cloned().collect();
    let mut last: Vec<String> = Vec::new();
    for attempt in 0..=retries {
        if attempt > 0 {
            #[cfg(target_os = "macos")]
            unsafe {
                unsafe extern "C" {
                    fn MIDIRestart() -> i32;
                }
                let _ = MIDIRestart();
            }
            std::thread::sleep(gap);
        }
        let infos = crate::enumerate::list_input_ports().unwrap_or_default();
        let current: Vec<String> = infos.iter().map(|i| i.raw_name.clone()).collect();
        log::info!(
            "adapter-midi: scan attempt {}/{} → {} port(s) visible {:?}",
            attempt,
            retries,
            current.len(),
            current
        );
        last = current.clone();
        for name in current {
            union.insert(name);
        }
        // Stop early if the union already grew beyond prev — we have
        // the new port, no need to keep restarting CoreMIDI.
        if union.len() > prev.len() {
            return union.into_iter().collect();
        }
    }
    // Exhausted retries without growth — return whatever the last
    // snapshot showed (could still differ from prev if a port was
    // removed).
    if last.is_empty() {
        union.into_iter().collect()
    } else {
        last
    }
}

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
/// `MidiCommand::PublishMidiEvent { source }` and the flag auto-clears via
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
                            let cmd = MidiCommand::PublishMidiEvent { source };
                            drop(bridge.submit(Command::Midi(cmd)));
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
    let mut connections: Vec<midir::MidiInputConnection<()>> = Vec::new();
    let mut known: Vec<String> = Vec::new();

    // Register the rescan channel and grab the receiver. The daemon
    // does ONE initial enumerate pass, then blocks on rescan_rx until
    // the GUI's refresh button (or any other caller) pulses
    // `adapter_midi::request_rescan()`. No timer, no polling — rescan
    // happens exactly when the user asks for it.
    let (rescan_tx, rescan_rx) = std::sync::mpsc::channel::<()>();
    crate::register_rescan_sender(rescan_tx);

    loop {
        // Drop every existing MidiInputConnection BEFORE we re-enumerate.
        // Each connection holds its own CoreMIDI client; the long-lived
        // clients were the ones blind to BLE-MIDI ports paired after
        // startup. By draining `connections` here we close those clients
        // — the next `list_input_ports()` call creates a fresh client
        // (inside `enumerate.rs`) that sees the current device list,
        // including any BLE port paired in the meantime. This is the
        // in-process equivalent of "reopen the app for MIDI only".
        let dropped = connections.len();
        connections.clear();
        if dropped > 0 {
            log::info!("adapter-midi: dropped {dropped} stale connection(s) before rescan");
        }

        let infos = crate::enumerate::list_input_ports().unwrap_or_default();
        let current: Vec<String> = infos.iter().map(|i| i.raw_name.clone()).collect();
        let added = new_port_names(&known, &current);
        log::info!(
            "adapter-midi rescan: {} port(s) visible {:?}; {} new since last scan {:?}",
            current.len(),
            current,
            added.len(),
            added,
        );
        for (idx, name) in current.iter().enumerate() {
            match attach_port(
                idx,
                name,
                Arc::clone(&profiles),
                Arc::clone(&selection),
                bridge.clone(),
                Arc::clone(&learn),
            ) {
                Ok(conn) => {
                    log::info!("adapter-midi: listening on '{name}' (profiles)");
                    connections.push(conn);
                }
                Err(e) => {
                    log::warn!("adapter-midi: failed to attach '{name}': {e}");
                }
            }
        }
        known = current;

        // Block until somebody calls adapter_midi::request_rescan().
        if rescan_rx.recv().is_err() {
            // All senders dropped; nothing else can wake us.
            return Ok(());
        }
    }
}

fn attach_port(
    idx: usize,
    port_name: &str,
    profiles: Arc<Vec<MidiProfile>>,
    selection: Arc<RwLock<SelectionState>>,
    bridge: CommandBridge,
    learn: Arc<LearnState>,
) -> Result<midir::MidiInputConnection<()>> {
    let client = MidiInput::new(CLIENT_NAME).context("creating MIDI input client")?;
    let ports = client.ports();
    let port = ports
        .get(idx)
        .ok_or_else(|| anyhow!("port index out of range"))?
        .clone();
    let name_for_cb = port_name.to_string();
    client
        .connect(
            &port,
            PORT_NAME,
            move |_stamp, bytes, _| {
                if learn.is_active() {
                    if let Some(source) = source_from_bytes(bytes) {
                        let cmd = MidiCommand::PublishMidiEvent { source };
                        drop(bridge.submit(Command::Midi(cmd)));
                        learn.on_event_captured();
                        return;
                    }
                    return;
                }
                let Some(msg) = IncomingMessage::from_bytes(bytes) else {
                    return;
                };
                let snapshot = match selection.read() {
                    Ok(g) => g.clone(),
                    Err(_) => return,
                };
                let active: Vec<&MidiProfile> = profiles.iter().collect();
                dispatch_midi_message_to_bridge(&active, &name_for_cb, &msg, &snapshot, &bridge);
            },
            (),
        )
        .map_err(|e| anyhow!("connecting to MIDI port '{port_name}': {e}"))
}

#[cfg(test)]
#[path = "daemon_tests.rs"]
mod tests;
