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
        src.contains("MidiCommand::SetMidiEnabled"),
        "issue #712: the MIDI master toggle must dispatch \
         MidiCommand::SetMidiEnabled (persists config.yaml midi_enabled with \
         GUI/MCP/gRPC parity), not mutate config directly in the callback."
    );
}

#[test]
fn integrations_wiring_dispatches_set_mcp_enabled() {
    let src = read_src("settings/integrations.rs");
    assert!(
        src.contains("SettingsCommand::SetMcpEnabled"),
        "issue #712: the MCP master toggle must dispatch \
         SettingsCommand::SetMcpEnabled, not mutate config directly in the callback."
    );
}

#[test]
fn integrations_wiring_syncs_the_in_memory_app_config_snapshot() {
    // Regression: the toggle persisted to config.yaml but came back OFF
    // after a restart. The GUI holds a boot-time `Rc<RefCell<AppConfig>>`
    // that recent-projects/project-open wirings re-save WHOLESALE
    // (`save_app_config(app_config.borrow().clone())`). A toggle that only
    // writes disk (and skips this shared snapshot) is clobbered by the
    // stale boot value the next time any project op fires. The fix mirrors
    // `settings::audio`: mutate `app_config.borrow_mut()` so the snapshot
    // carries the new value.
    let src = read_src("settings/integrations.rs");
    assert!(
        src.contains("app_config") && src.contains("borrow_mut"),
        "issue #712: the toggle must also update the shared in-memory \
         AppConfig snapshot (mutate `app_config.borrow_mut()`), else the \
         recent-projects wiring re-saves the stale boot snapshot and the \
         switch resets on restart. See settings::audio for the pattern."
    );
}
