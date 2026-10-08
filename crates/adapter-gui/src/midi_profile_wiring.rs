//! Responsibility: starts the profile-driven MIDI daemon.
//! GUI entry point for the profile-driven MIDI daemon.
//!
//! The binary calls `start_midi_profiles` once, after the dispatcher
//! is built. The daemon then runs on its own thread, opening every
//! available MIDI input, parsing each message through
//! `adapter_midi::IncomingMessage::from_bytes`, matching it against
//! every profile loaded from disk (factory + user dirs), and submitting
//! the resulting `Command`s over the bridge — the GUI's drain loop
//! runs them like a click would.
//!
//! Profile dirs:
//! - **factory** = `<data-root>/assets/midi-profiles/` (the install's
//!   shipped profiles; `data-root` resolved by `infra_filesystem::detect_data_root`).
//! - **user** = `<user folder>/midi-profiles/` (`infra_filesystem::user_data_root`:
//!   `~/.openrig` on macOS and Linux, `%APPDATA%\OpenRig` on Windows). Drop a
//!   `<name>.yaml` here and the daemon picks it up on the next launch — no
//!   rebuild.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use application::bridge::CommandBridge;
use application::SelectionState;

fn factory_profiles_dir() -> PathBuf {
    infra_filesystem::detect_data_root().join("assets/midi-profiles")
}

fn user_profiles_dir() -> PathBuf {
    infra_filesystem::user_data_root().join("midi-profiles")
}

pub fn start_midi_profiles(
    bridge: CommandBridge,
    selection: Arc<RwLock<SelectionState>>,
    learn: Arc<adapter_midi::learn::LearnState>,
) -> JoinHandle<anyhow::Result<()>> {
    let factory = factory_profiles_dir();
    let user = user_profiles_dir();
    log::info!(
        "midi profile dirs — factory: {} — user: {}",
        factory.display(),
        user.display()
    );
    adapter_midi::spawn_with_profiles_from(&factory, &user, bridge, selection, learn)
}

#[cfg(test)]
#[path = "midi_profile_wiring_tests.rs"]
mod tests;
