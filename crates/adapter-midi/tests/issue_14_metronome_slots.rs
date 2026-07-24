//! Red-first (#14): the metronome's two MIDI slots.
//!
//! `toggle_metronome` follows the same contract as `toggle_tuner` — read the
//! current state from `SelectionState`, dispatch the inverted value — so a
//! footswitch stays in sync with the on-screen power switch.
//!
//! `metronome_tap` is the one slot that carries no state: every press is a tap,
//! and the adapter turns the tap history into a tempo. Tapping a footswitch is
//! the natural way to set tempo on stage, so it has to reach the same door the
//! GUI button does.

use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{Command, MetronomeCommand};
use application::SelectionState;

fn any_msg() -> IncomingMessage {
    IncomingMessage::NoteOn {
        channel: 1,
        note: 60,
        velocity: 100,
    }
}

#[test]
fn toggle_metronome_flips_current_state() {
    let mut sel = SelectionState::default();
    assert!(
        !sel.metronome_enabled,
        "the metronome starts off, like every other global toggle"
    );

    let cmd = slot_to_command("toggle_metronome", &any_msg(), &sel)
        .expect("toggle_metronome should resolve to a Command");
    match cmd {
        Command::Metronome(MetronomeCommand::SetMetronomeEnabled { enabled }) => {
            assert!(enabled, "off → on");
        }
        other => panic!("expected SetMetronomeEnabled, got {other:?}"),
    }

    sel.metronome_enabled = true;
    let cmd = slot_to_command("toggle_metronome", &any_msg(), &sel)
        .expect("toggle_metronome should resolve to a Command");
    match cmd {
        Command::Metronome(MetronomeCommand::SetMetronomeEnabled { enabled }) => {
            assert!(!enabled, "on → off");
        }
        other => panic!("expected SetMetronomeEnabled, got {other:?}"),
    }
}

#[test]
fn metronome_tap_is_stateless() {
    let mut sel = SelectionState::default();

    for enabled in [false, true] {
        sel.metronome_enabled = enabled;
        let cmd = slot_to_command("metronome_tap", &any_msg(), &sel)
            .expect("metronome_tap should resolve to a Command");
        assert!(
            matches!(cmd, Command::Metronome(MetronomeCommand::MetronomeTap)),
            "every press is a tap regardless of state, got {cmd:?}"
        );
    }
}
