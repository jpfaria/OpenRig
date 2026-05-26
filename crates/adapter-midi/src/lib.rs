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
pub mod profile;
pub mod resolve;
mod translate;

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod resolve_tests;

#[cfg(test)]
#[path = "learn_tests.rs"]
mod learn_tests;

pub use daemon::{run_blocking, run_blocking_with_map};
pub use enumerate::{list_input_ports, MidiPortInfo, MidiPortKey};
pub use learn::{learn_state, LearnState};
pub use mapping::{Binding, MidiMap, Scale, Source};
pub use message::MidiMessage;
pub use resolve::resolve_midi_map;
pub use translate::resolve as translate_message;
