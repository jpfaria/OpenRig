//! Phase 3c red-first test (issue #548): pure function that maps a
//! catalog slot name + MIDI message + current SelectionState → Command.
//!
//! The function is **pure** so it's trivially testable without a
//! dispatcher mock. The MIDI daemon (Phase 4) calls it and forwards the
//! returned Command to `LocalDispatcher`.

use adapter_midi::profile::MatchExpr;
use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{ChainId, Command, RigNavKind, SelectionCommand};
use application::SelectionState;

fn pc(channel: u8, program: u8) -> IncomingMessage {
    IncomingMessage::ProgramChange { channel, program }
}

fn selection_with_chain(chain: &str) -> SelectionState {
    SelectionState {
        active_chain: Some(chain.to_string()),
        ..SelectionState::default()
    }
}

#[test]
fn prev_preset_emits_step_preset_minus_1_on_active_chain() {
    let cmd = slot_to_command(
        "prev_preset",
        &pc(1, 0),
        &selection_with_chain("rig:guitar"),
    )
    .expect("slot returns a Command");
    match cmd {
        Command::Selection(SelectionCommand::ApplyRigNav { chain, kind }) => {
            assert_eq!(chain, ChainId("rig:guitar".to_string()));
            assert!(matches!(kind, RigNavKind::StepPreset(-1)));
        }
        other => panic!("expected ApplyRigNav, got {other:?}"),
    }
}

#[test]
fn next_preset_emits_step_preset_plus_1() {
    let cmd = slot_to_command(
        "next_preset",
        &pc(1, 1),
        &selection_with_chain("rig:guitar"),
    )
    .expect("slot returns a Command");
    assert!(matches!(
        cmd,
        Command::Selection(SelectionCommand::ApplyRigNav {
            kind: RigNavKind::StepPreset(1),
            ..
        })
    ));
}

#[test]
fn prev_next_scene_emit_step_scene() {
    for (slot, expected_delta) in [("prev_scene", -1i32), ("next_scene", 1)] {
        let cmd = slot_to_command(slot, &pc(1, 2), &selection_with_chain("g")).unwrap();
        if let Command::Selection(SelectionCommand::ApplyRigNav {
            kind: RigNavKind::StepScene(d),
            ..
        }) = cmd
        {
            assert_eq!(d, expected_delta, "slot {slot}");
        } else {
            panic!("expected StepScene for {slot}");
        }
    }
}

#[test]
fn jump_preset_n_uses_program_value_as_index() {
    let cmd = slot_to_command(
        "jump_preset_n",
        &pc(1, 42),
        &selection_with_chain("rig:guitar"),
    )
    .unwrap();
    assert!(matches!(
        cmd,
        Command::Selection(SelectionCommand::ApplyRigNav {
            kind: RigNavKind::Preset(42),
            ..
        })
    ));
}

#[test]
fn jump_scene_n_uses_program_value_as_index() {
    let cmd = slot_to_command("jump_scene_n", &pc(1, 7), &selection_with_chain("g")).unwrap();
    assert!(matches!(
        cmd,
        Command::Selection(SelectionCommand::ApplyRigNav {
            kind: RigNavKind::Scene(7),
            ..
        })
    ));
}

#[test]
fn prev_next_chain_emit_select_active_chain_relative() {
    for (slot, expected) in [("prev_chain", -1i32), ("next_chain", 1)] {
        let cmd = slot_to_command(slot, &pc(1, 0), &SelectionState::default()).unwrap();
        if let Command::Selection(SelectionCommand::SelectActiveChainRelative { delta }) = cmd {
            assert_eq!(delta, expected, "slot {slot}");
        } else {
            panic!("expected SelectActiveChainRelative for {slot}");
        }
    }
}

#[test]
fn prev_next_block_1_and_2_emit_select_active_block_relative() {
    for (slot, expected) in [
        ("prev_block_1", -1i32),
        ("next_block_1", 1),
        ("prev_block_2", -2),
        ("next_block_2", 2),
    ] {
        let cmd = slot_to_command(slot, &pc(1, 0), &SelectionState::default()).unwrap();
        if let Command::Selection(SelectionCommand::SelectActiveBlockRelative { delta }) = cmd {
            assert_eq!(delta, expected, "slot {slot}");
        } else {
            panic!("expected SelectActiveBlockRelative for {slot}");
        }
    }
}

#[test]
fn toggle_compact_view_inverts_selection_flag() {
    let mut sel = SelectionState {
        compact_view_enabled: false,
        ..Default::default()
    };
    let cmd = slot_to_command("toggle_compact_view", &pc(1, 0), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetCompactViewEnabled { enabled }) = cmd {
        assert!(enabled, "off → on");
    } else {
        panic!("expected SetCompactViewEnabled");
    }

    sel.compact_view_enabled = true;
    let cmd = slot_to_command("toggle_compact_view", &pc(1, 0), &sel).unwrap();
    if let Command::Selection(SelectionCommand::SetCompactViewEnabled { enabled }) = cmd {
        assert!(!enabled, "on → off");
    } else {
        panic!("expected SetCompactViewEnabled");
    }
}

#[test]
fn unknown_slot_returns_none() {
    assert!(
        slot_to_command("not_a_real_slot", &pc(1, 0), &SelectionState::default()).is_none(),
        "unknown slot should return None, not panic"
    );
}

#[test]
fn matchexpr_is_used_in_module_for_future_extensions() {
    // Compile-time only: prove the public type used by the daemon to
    // build IncomingMessage from a profile match is the same module's
    // MatchExpr — pinning the import surface.
    let _: MatchExpr = MatchExpr::ProgramChange {
        channel: 1,
        program: Some(0),
    };
}
