//! #913 — flipping a subsystem master switch.
//!
//! Two things have to happen together. The in-memory snapshot must move, or
//! the next wholesale config save (opening a project, recording a recent) puts
//! the old value back and the user sees the switch reset on restart. And the
//! change must reach the dispatcher whenever there IS one, so a client sees the
//! same state the screen shows.

use super::integrations_toggle::record_toggle;
use crate::state::ProjectSession;
use application::command::{Command, MidiCommand, SettingsCommand};
use infra_filesystem::AppConfig;
use project::project::Project;
use std::cell::RefCell;
use std::rc::Rc;

fn open_session() -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(Some(ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-integrations-tests"),
    ))))
}

fn config() -> Rc<RefCell<AppConfig>> {
    Rc::new(RefCell::new(AppConfig::default()))
}

fn midi_command(enabled: bool) -> Command {
    Command::Midi(MidiCommand::SetMidiEnabled { enabled })
}

fn mcp_command(enabled: bool) -> Command {
    Command::Settings(SettingsCommand::SetMcpEnabled { enabled })
}

#[test]
fn the_shared_snapshot_is_mirrored_so_a_later_wholesale_save_cannot_undo_it() {
    let config = config();
    record_toggle(
        &open_session(),
        &config,
        |c, on| c.midi_enabled = on,
        midi_command,
        true,
    );
    assert!(
        config.borrow().midi_enabled,
        "without the mirror the next project-open save resets the switch"
    );
}

#[test]
fn with_a_project_open_the_toggle_goes_on_the_bus() {
    assert!(record_toggle(
        &open_session(),
        &config(),
        |c, on| c.midi_enabled = on,
        midi_command,
        true,
    ));
}

#[test]
fn from_the_launcher_the_caller_is_told_to_persist_directly() {
    let none: Rc<RefCell<Option<ProjectSession>>> = Rc::new(RefCell::new(None));
    let config = config();
    assert!(
        !record_toggle(
            &none,
            &config,
            |c, on| c.mcp_enabled = on,
            mcp_command,
            true
        ),
        "no dispatcher at the launcher — the caller writes config.yaml itself"
    );
    assert!(
        config.borrow().mcp_enabled,
        "the snapshot still moves, dispatcher or not"
    );
}

#[test]
fn switching_off_is_recorded_the_same_way() {
    let config = config();
    config.borrow_mut().midi_enabled = true;
    record_toggle(
        &open_session(),
        &config,
        |c, on| c.midi_enabled = on,
        midi_command,
        false,
    );
    assert!(!config.borrow().midi_enabled);
}

#[test]
fn each_switch_writes_only_its_own_field() {
    let config = config();
    let session = open_session();
    record_toggle(
        &session,
        &config,
        |c, on| c.midi_enabled = on,
        midi_command,
        true,
    );
    assert!(config.borrow().midi_enabled);
    assert!(
        !config.borrow().mcp_enabled,
        "flipping MIDI must not turn the MCP server on"
    );
}
