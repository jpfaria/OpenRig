//! Frontend-agnostic MIDI/BLE-MIDI controller adapter (issue #22).
//!
//! Owns no state. A controller (USB or BLE-MIDI footswitch, expression
//! pedal — e.g. the M-Vave Chocolate) drives the **same** `Command`s the GUI
//! uses: incoming MIDI is matched against `midi-map.yaml`, turned into a typed
//! `application::command::Command`, and submitted over
//! `application::bridge::CommandBridge` exactly as an MCP tool call is. The
//! frontend drains and dispatches on its own thread — zero audio-thread
//! impact, real-time invariants preserved by construction.

mod daemon;
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

/// One-call helper for the adapter that wants every bundled factory
/// profile active out of the box (the GUI default). Spawns the MIDI
/// daemon on a fresh thread and returns its `JoinHandle`. The thread
/// runs forever (the daemon is a `loop { park }`); the handle is mostly
/// for the call site to keep alive and observe panics.
pub fn spawn_with_bundled_profiles(
    bridge: application::bridge::CommandBridge,
    selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
    learn: std::sync::Arc<learn::LearnState>,
) -> std::thread::JoinHandle<anyhow::Result<()>> {
    let profiles = profile::load_bundled_profiles();
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
