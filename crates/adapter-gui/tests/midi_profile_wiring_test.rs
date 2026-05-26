//! Phase 6 red-first (issue #548): adapter-gui must expose a single
//! entry point the binary's main calls to start the profile-driven
//! MIDI daemon (replacing the legacy single-file map path).
//!
//! Signature-only test — driving real MIDI input is out of scope here;
//! the daemon itself is covered by `adapter-midi`'s own tests.

#[test]
fn start_midi_profiles_signature_exists() {
    use std::sync::{Arc, RwLock};
    use std::thread::JoinHandle;

    let _f: fn(
        application::bridge::CommandBridge,
        Arc<RwLock<application::SelectionState>>,
        Arc<adapter_midi::learn::LearnState>,
    ) -> JoinHandle<anyhow::Result<()>> = adapter_gui::start_midi_profiles;
}
