//! Issue #712: the System / Integrations section toggles must dispatch the
//! per-machine master switches through the command bus — never mutate
//! config or borrow_mut in the callback (the GUI-has-no-business-logic
//! LAW). UI rendering can't be asserted without an `AppWindow`, so this is
//! a source-presence guard on the wiring glue: it flips GREEN once the
//! callbacks dispatch `SetMidiEnabled` / `SetMcpEnabled`, RED on any
//! regression that bypasses the dispatcher.

use std::path::PathBuf;

fn read_src(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn integrations_wiring_dispatches_set_midi_enabled() {
    let src = read_src("settings/integrations.rs");
    assert!(
        src.contains("Command::SetMidiEnabled"),
        "issue #712: the MIDI master toggle must dispatch \
         Command::SetMidiEnabled (persists config.yaml midi_enabled with \
         GUI/MCP/gRPC parity), not mutate config directly in the callback."
    );
}

#[test]
fn integrations_wiring_dispatches_set_mcp_enabled() {
    let src = read_src("settings/integrations.rs");
    assert!(
        src.contains("Command::SetMcpEnabled"),
        "issue #712: the MCP master toggle must dispatch \
         Command::SetMcpEnabled, not mutate config directly in the callback."
    );
}
