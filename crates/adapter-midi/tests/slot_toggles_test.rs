//! Phase 3d red-first test (issue #548): the 3 global toggles
//! (toggle_tuner, toggle_output_mute, toggle_spectrum) read the current
//! state from `SelectionState` and dispatch the matching `Set*` Command
//! with the inverted value.
//!
//! Requires extending `SelectionState` with three flags the GUI keeps in
//! sync (already does for the tuner/mute/spectrum buttons it renders).

use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{Command, SelectionCommand};
use application::SelectionState;

fn any_msg() -> IncomingMessage {
    IncomingMessage::NoteOn {
        channel: 1,
        note: 60,
        velocity: 100,
    }
}

#[test]
fn toggle_tuner_flips_current_state() {
    let mut sel = SelectionState::default();
    assert!(!sel.tuner_enabled);
    let cmd = slot_to_command("toggle_tuner", &any_msg(), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetTunerEnabled { enabled }) = cmd {
        assert!(enabled, "off → on");
    } else {
        panic!("expected SetTunerEnabled");
    }

    sel.tuner_enabled = true;
    let cmd = slot_to_command("toggle_tuner", &any_msg(), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetTunerEnabled { enabled }) = cmd {
        assert!(!enabled, "on → off");
    } else {
        panic!("expected SetTunerEnabled");
    }
}

#[test]
fn toggle_output_mute_flips_current_state() {
    let mut sel = SelectionState::default();
    assert!(!sel.output_muted);
    let cmd = slot_to_command("toggle_output_mute", &any_msg(), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetOutputMuted { muted }) = cmd {
        assert!(muted, "off → on");
    } else {
        panic!("expected SetOutputMuted");
    }

    sel.output_muted = true;
    let cmd = slot_to_command("toggle_output_mute", &any_msg(), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetOutputMuted { muted }) = cmd {
        assert!(!muted, "on → off");
    } else {
        panic!("expected SetOutputMuted");
    }
}

#[test]
fn toggle_spectrum_flips_current_state() {
    let mut sel = SelectionState::default();
    assert!(!sel.spectrum_enabled);
    let cmd = slot_to_command("toggle_spectrum", &any_msg(), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetSpectrumEnabled { enabled }) = cmd {
        assert!(enabled, "off → on");
    } else {
        panic!("expected SetSpectrumEnabled");
    }

    sel.spectrum_enabled = true;
    let cmd = slot_to_command("toggle_spectrum", &any_msg(), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetSpectrumEnabled { enabled }) = cmd {
        assert!(!enabled, "on → off");
    } else {
        panic!("expected SetSpectrumEnabled");
    }
}
