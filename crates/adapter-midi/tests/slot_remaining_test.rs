//! Phase 3e red-first test (issue #548): 4 remaining slots.
//!
//! - toggle_active_chain_enabled, toggle_active_block_enabled — read
//!   live snapshot from SelectionState, dispatch the matching toggle
//!   Command with the inverted value.
//! - chain_volume — dispatch SetChainVolume on the active chain with
//!   the CC value scaled 0..127 → 0.0..1.0 (the project layer maps the
//!   normalised value to the actual chain-volume range).
//! - block_param_numeric — dispatch SetBlockParameterNumber on the
//!   active chain/block; param path picked by the GUI snapshot
//!   (active_block_param_path); value is the normalised CC byte.

use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{BlockCommand, BlockId, ChainCommand, ChainId, Command};
use application::SelectionState;

fn cc(value: u8) -> IncomingMessage {
    IncomingMessage::ControlChange {
        channel: 1,
        controller: 7,
        value,
    }
}

#[test]
fn toggle_active_chain_enabled_flips_snapshot() {
    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        active_chain_enabled: true,
        ..Default::default()
    };

    let cmd = slot_to_command("toggle_active_chain_enabled", &cc(0), &sel).unwrap();
    match cmd {
        Command::Chain(ChainCommand::ToggleChainEnabled { chain }) => {
            assert_eq!(chain, ChainId("rig:guitar".to_string()));
        }
        other => panic!("expected ToggleChainEnabled, got {other:?}"),
    }
}

#[test]
fn toggle_active_chain_enabled_is_none_without_active_chain() {
    let sel = SelectionState::default();
    assert!(slot_to_command("toggle_active_chain_enabled", &cc(0), &sel).is_none());
}

#[test]
fn toggle_active_block_enabled_emits_toggle_for_both_ids() {
    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        active_block: Some("blk_1".to_string()),
        ..Default::default()
    };

    let cmd = slot_to_command("toggle_active_block_enabled", &cc(0), &sel).unwrap();
    match cmd {
        Command::Block(BlockCommand::ToggleBlockEnabled { chain, block }) => {
            assert_eq!(chain, ChainId("rig:guitar".to_string()));
            assert_eq!(block, BlockId("blk_1".to_string()));
        }
        other => panic!("expected ToggleBlockEnabled, got {other:?}"),
    }
}

#[test]
fn toggle_active_block_enabled_is_none_without_chain_or_block() {
    let sel = SelectionState {
        active_chain: Some("c".to_string()),
        ..Default::default()
    };
    // no active_block
    assert!(slot_to_command("toggle_active_block_enabled", &cc(0), &sel).is_none());

    let sel2 = SelectionState {
        active_block: Some("b".to_string()),
        ..Default::default()
    };
    // no active_chain
    assert!(slot_to_command("toggle_active_block_enabled", &cc(0), &sel2).is_none());
}

#[test]
fn chain_volume_scales_cc_zero_to_zero() {
    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        ..Default::default()
    };
    let cmd = slot_to_command("chain_volume", &cc(0), &sel).unwrap();
    match cmd {
        Command::Chain(ChainCommand::SetChainVolume { chain, value }) => {
            assert_eq!(chain, ChainId("rig:guitar".to_string()));
            assert!((value - 0.0).abs() < 1e-6, "got {value}");
        }
        other => panic!("expected SetChainVolume, got {other:?}"),
    }
}

#[test]
fn chain_volume_scales_cc_127_to_one() {
    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        ..Default::default()
    };
    let cmd = slot_to_command("chain_volume", &cc(127), &sel).unwrap();
    if let Command::Chain(ChainCommand::SetChainVolume { value, .. }) = cmd {
        assert!((value - 1.0).abs() < 1e-6, "got {value}");
    } else {
        panic!("expected SetChainVolume");
    }
}

#[test]
fn chain_volume_scales_cc_64_to_about_half() {
    let sel = SelectionState {
        active_chain: Some("g".to_string()),
        ..Default::default()
    };
    let cmd = slot_to_command("chain_volume", &cc(64), &sel).unwrap();
    if let Command::Chain(ChainCommand::SetChainVolume { value, .. }) = cmd {
        assert!((value - 64.0 / 127.0).abs() < 1e-6, "got {value}");
    } else {
        panic!("expected SetChainVolume");
    }
}

#[test]
fn chain_volume_is_none_without_active_chain() {
    let sel = SelectionState::default();
    assert!(slot_to_command("chain_volume", &cc(64), &sel).is_none());
}

#[test]
fn block_param_numeric_uses_active_block_and_path() {
    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        active_block: Some("blk_1".to_string()),
        active_block_param_path: Some("gain".to_string()),
        ..Default::default()
    };

    let cmd = slot_to_command("block_param_numeric", &cc(127), &sel).unwrap();
    match cmd {
        Command::Block(BlockCommand::SetBlockParameterNumber {
            chain,
            block,
            path,
            value,
        }) => {
            assert_eq!(chain, ChainId("rig:guitar".to_string()));
            assert_eq!(block, BlockId("blk_1".to_string()));
            assert_eq!(path, "gain");
            assert!((value - 1.0).abs() < 1e-6);
        }
        other => panic!("expected SetBlockParameterNumber, got {other:?}"),
    }
}

#[test]
fn block_param_numeric_none_when_path_missing() {
    let sel = SelectionState {
        active_chain: Some("c".to_string()),
        active_block: Some("b".to_string()),
        ..Default::default()
    };
    // no active_block_param_path
    assert!(slot_to_command("block_param_numeric", &cc(64), &sel).is_none());
}
