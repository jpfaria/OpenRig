//! The drum machine's footswitch slots: play/stop and fill.

use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{Command, DrumsCommand};
use application::SelectionState;

fn any_msg() -> IncomingMessage {
    IncomingMessage::NoteOn {
        channel: 1,
        note: 60,
        velocity: 100,
    }
}

#[test]
fn toggle_drums_asks_the_dispatcher_to_flip_the_transport() {
    let cmd = slot_to_command("toggle_drums", &any_msg(), &SelectionState::default())
        .expect("toggle_drums resolves");
    assert!(matches!(cmd, Command::Drums(DrumsCommand::ToggleDrums)));
}

#[test]
fn drum_fill_triggers_a_fill() {
    let cmd = slot_to_command("drum_fill", &any_msg(), &SelectionState::default())
        .expect("drum_fill resolves");
    assert!(matches!(cmd, Command::Drums(DrumsCommand::TriggerDrumFill)));
}
