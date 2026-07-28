//! Red-first (issue #548): new slot `toggle_active_block_neighbor_enabled`
//! toggles the block *after* the active one in the chain — the second
//! block of the compact-view pair. Wraps to index 0 when active_block
//! is the last in the chain.

use adapter_midi::profile::{parse_profile_yaml, CATALOG};
use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{Command, SelectionCommand};
use application::SelectionState;

#[test]
fn catalog_includes_neighbor_toggle() {
    assert!(
        CATALOG.contains(&"toggle_active_block_neighbor_enabled"),
        "CATALOG missing the new slot — got {CATALOG:?}"
    );
}

#[test]
fn parser_accepts_neighbor_slot() {
    let yaml = r#"
name: "neighbor"
bindings:
  - when: { kind: ProgramChange, channel: 1, program: 10 }
    do: toggle_active_block_neighbor_enabled
"#;
    parse_profile_yaml(yaml).expect("must accept");
}

#[test]
fn slot_returns_none_without_active_chain_or_block() {
    let msg = IncomingMessage::ProgramChange {
        channel: 1,
        program: 0,
    };
    assert!(slot_to_command(
        "toggle_active_block_neighbor_enabled",
        &msg,
        &SelectionState::default(),
    )
    .is_none());
}

#[test]
fn slot_returns_command_for_neighbor_when_chain_and_block_active() {
    // Slot is a pure function — it can't see project.chains.blocks, so
    // the resolver returns `ToggleBlockEnabled` with the **same** block
    // id from SelectionState as a sentinel. The real "neighbor lookup"
    // lives in the dispatcher when it handles the resulting Command;
    // here we just prove the slot reaches the dispatch path with an
    // active block present.
    let sel = SelectionState {
        active_chain: Some("rig:guitar".to_string()),
        active_block: Some("blk_a".to_string()),
        ..Default::default()
    };
    let msg = IncomingMessage::ProgramChange {
        channel: 1,
        program: 0,
    };
    let cmd = slot_to_command("toggle_active_block_neighbor_enabled", &msg, &sel)
        .expect("must dispatch when chain+block active");
    // We deliberately don't pin the block id — neighbor resolution is
    // dispatcher-side. The slot must dispatch *some* ToggleBlockEnabled
    // with the active chain so the dispatcher can resolve "next block"
    // from the project.
    match cmd {
        Command::Selection(SelectionCommand::ToggleActiveBlockNeighborEnabled) => {}
        other => panic!("expected ToggleActiveBlockNeighborEnabled, got {other:?}"),
    }
}
