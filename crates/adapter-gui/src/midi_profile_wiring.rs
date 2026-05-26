//! GUI entry point for the profile-driven MIDI daemon (issue #548).
//!
//! The binary calls `start_midi_profiles` once, after the dispatcher
//! is built. The daemon then runs on its own thread, opening every
//! available MIDI input, parsing each message through
//! `adapter_midi::IncomingMessage::from_bytes`, matching it against
//! every bundled profile, and submitting the resulting `Command`s over
//! the bridge — the GUI's drain loop runs them like a click would.

use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use application::bridge::CommandBridge;
use application::SelectionState;

pub fn start_midi_profiles(
    bridge: CommandBridge,
    selection: Arc<RwLock<SelectionState>>,
    learn: Arc<adapter_midi::learn::LearnState>,
) -> JoinHandle<anyhow::Result<()>> {
    adapter_midi::spawn_with_bundled_profiles(bridge, selection, learn)
}
