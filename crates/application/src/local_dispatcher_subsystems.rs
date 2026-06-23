//! #712 — `Command::SetMidiEnabled` / `SetMcpEnabled`: per-machine master
//! switches for the MIDI/BLE-MIDI adapter and the MCP server.
//!
//! Enablement is system config (ADR 0003), not a CLI flag: packaged
//! builds launch the binary with no arguments, so `--midi`/`--mcp` never
//! reached the installed app. These handlers persist the flag into
//! `config.yaml` (the read-modify-write runs on the persist worker so the
//! dispatching thread never waits on disk, ordered with every other config
//! write), touching ONLY the one field and preserving the rest of
//! `AppConfig`. The subsystem is wired at bootstrap, so the change takes
//! effect on next launch — the emitted event lets the GUI surface a
//! restart hint.

use anyhow::Result;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_set_midi_enabled(&self, enabled: bool) -> Result<Vec<Event>> {
        // #731: bind the config path at dispatch time (see app_config_persist).
        crate::app_config_persist::persist_app_config(move |config| config.midi_enabled = enabled);
        Ok(vec![Event::MidiEnabledChanged { enabled }])
    }

    pub(crate) fn handle_set_mcp_enabled(&self, enabled: bool) -> Result<Vec<Event>> {
        // #731: bind the config path at dispatch time (see app_config_persist).
        crate::app_config_persist::persist_app_config(move |config| config.mcp_enabled = enabled);
        Ok(vec![Event::McpEnabledChanged { enabled }])
    }
}
