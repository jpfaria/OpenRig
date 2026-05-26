//! Phase 5 wiring red-first (issue #548): pipeline-to-bridge connector
//! exists with the expected signature so the daemon callback can
//! borrow it. We don't drive a CommandBridge here (constructor is
//! adapter-private); the function's behaviour shares its match→slot
//! path with dispatch_midi_message, which is already covered end-to-end
//! by dispatch_integration_test.rs.

#[test]
fn dispatch_midi_message_to_bridge_signature() {
    let _f: fn(
        &[&adapter_midi::profile::MidiProfile],
        &str,
        &adapter_midi::slots::IncomingMessage,
        &application::SelectionState,
        &application::bridge::CommandBridge,
    ) = adapter_midi::pipeline::dispatch_midi_message_to_bridge;
}

#[test]
fn run_blocking_with_profiles_signature() {
    use std::sync::{Arc, RwLock};

    let _f: fn(
        application::bridge::CommandBridge,
        Vec<adapter_midi::profile::MidiProfile>,
        Arc<RwLock<application::SelectionState>>,
        Arc<adapter_midi::learn::LearnState>,
    ) -> anyhow::Result<()> = adapter_midi::run_blocking_with_profiles;
}

#[test]
fn spawn_with_bundled_profiles_signature() {
    use std::sync::{Arc, RwLock};
    use std::thread::JoinHandle;

    let _f: fn(
        application::bridge::CommandBridge,
        Arc<RwLock<application::SelectionState>>,
        Arc<adapter_midi::learn::LearnState>,
    ) -> JoinHandle<anyhow::Result<()>> = adapter_midi::spawn_with_bundled_profiles;
}
