//! Frontend-agnostic MIDI/BLE-MIDI controller adapter (issue #22).
//!
//! Owns no state. A controller (USB or BLE-MIDI footswitch, expression
//! pedal — e.g. the M-Vave Chocolate) drives the **same** `Command`s the GUI
//! uses: incoming MIDI is matched against `midi-map.yaml`, turned into a typed
//! `application::command::Command`, and submitted over
//! `application::bridge::CommandBridge` exactly as an MCP tool call is. The
//! frontend drains and dispatches on its own thread — zero audio-thread
//! impact, real-time invariants preserved by construction.

pub mod daemon;
pub mod enumerate;
pub mod learn;
mod mapping;
mod message;
pub mod pipeline;
pub mod profile;
pub mod resolve;
pub mod slots;
mod translate;

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod resolve_tests;

#[cfg(test)]
#[path = "learn_tests.rs"]
mod learn_tests;

pub use daemon::{run_blocking, run_blocking_with_map, run_blocking_with_profiles};

/// Shared rescan signal. The MIDI daemon parks on a `Receiver<()>`;
/// the GUI's Settings "refresh" button calls [`request_rescan`] to
/// pulse it. No timer, no polling — the rescan happens exactly when
/// the user asks for it.
static RESCAN_TX: std::sync::OnceLock<std::sync::mpsc::Sender<()>> = std::sync::OnceLock::new();

/// Signal the daemon to re-enumerate MIDI inputs and attach any new
/// ports. Safe to call before the daemon starts — silently no-op when
/// nobody is listening.
pub fn request_rescan() {
    if let Some(tx) = RESCAN_TX.get() {
        let _ = tx.send(());
    }
}

/// Register the daemon's rescan receiver. Internal; the daemon calls
/// this once on startup. Subsequent calls are ignored (the `OnceLock`
/// guarantees a single owner).
pub(crate) fn register_rescan_sender(tx: std::sync::mpsc::Sender<()>) {
    let _ = RESCAN_TX.set(tx);
}

/// One-call helper for the adapter that wants every profile from
/// disk active out of the box. Scans `factory_dir` (install assets,
/// e.g. the repo's `assets/midi-profiles/`) and `user_dir` (the user's
/// per-app data dir, e.g. `~/.local/share/openrig/midi-profiles/`),
/// concatenates everything, and spawns the daemon on a fresh thread.
/// Either dir may be missing — empty contributes nothing.
pub fn spawn_with_profiles_from(
    factory_dir: &std::path::Path,
    user_dir: &std::path::Path,
    bridge: application::bridge::CommandBridge,
    selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
    learn: std::sync::Arc<learn::LearnState>,
) -> std::thread::JoinHandle<anyhow::Result<()>> {
    let mut profiles = profile::load_profiles_from_dir(factory_dir);
    profiles.extend(profile::load_profiles_from_dir(user_dir));
    log::info!(
        "adapter-midi: {} factory + user profile(s) loaded ({} from {}, {} from {})",
        profiles.len(),
        profile::load_profiles_from_dir(factory_dir).len(),
        factory_dir.display(),
        profile::load_profiles_from_dir(user_dir).len(),
        user_dir.display(),
    );
    std::thread::Builder::new()
        .name("openrig-midi-profiles".into())
        .spawn(move || run_blocking_with_profiles(bridge, profiles, selection, learn))
        .expect("spawn midi-profiles thread")
}
pub use enumerate::{list_input_ports, MidiPortInfo, MidiPortKey};
pub use learn::{learn_state, LearnState};
pub use mapping::{Binding, MidiMap, Scale, Source};
pub use message::MidiMessage;
pub use resolve::resolve_midi_map;
pub use translate::resolve as translate_message;
