use super::*;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn io_block(id: &str, input: bool) -> AudioBlock {
    // #716: I/O blocks no longer embed device endpoints; their device data
    // lives in the binding registry. These tests only exercise selection
    // index math, so the io/endpoint fields stay empty.
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: if input {
            AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                io: String::new(),
                endpoint: String::new(),
            })
        } else {
            AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                io: String::new(),
                endpoint: String::new(),
            })
        },
    }
}

fn core_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".to_string(),
            model: "test".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        // Input, b0, b1, Output — UI strip is [b0, b1] (IO stripped).
        blocks: vec![
            io_block("in", true),
            core_block("b0"),
            core_block("b1"),
            io_block("out", false),
        ],
        di_output: None,
    }
}

fn project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:input-1"), chain("rig:input-3")],
        midi: None,
    }
}

#[test]
fn no_active_chain_marks_nothing() {
    let sel = SelectionState::default();
    assert_eq!(active_highlight_indices(&project(), &sel), (-1, -1));
}

#[test]
fn active_chain_without_block_marks_the_row_only() {
    let sel = SelectionState {
        active_chain: Some("rig:input-3".to_string()),
        ..Default::default()
    };
    // index 1, no block → block UI index -1
    assert_eq!(active_highlight_indices(&project(), &sel), (1, -1));
}

#[test]
fn active_chain_and_block_marks_both_with_ui_block_index() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b1".to_string()),
        ..Default::default()
    };
    // chain 0; "b1" is the 2nd core block → UI index 1 (IO stripped).
    assert_eq!(active_highlight_indices(&project(), &sel), (0, 1));
}

#[test]
fn stale_active_chain_marks_nothing() {
    let sel = SelectionState {
        active_chain: Some("rig:does-not-exist".to_string()),
        ..Default::default()
    };
    assert_eq!(active_highlight_indices(&project(), &sel), (-1, -1));
}

// ── neighbor block (the block `toggle_active_block_neighbor_enabled` acts on) ──

#[test]
fn neighbor_is_the_next_ui_block() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()), // UI 0
        ..Default::default()
    };
    // neighbor = the block after the active one → b1 (UI 1)
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), 1);
}

#[test]
fn neighbor_is_minus_one_when_next_block_is_io() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b1".to_string()), // last audio block; raw-next is Output
        ..Default::default()
    };
    // The toggle-neighbor command targets the raw-next block (here the
    // Output endpoint), which has no chip on the strip → not markable.
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), -1);
}

#[test]
fn neighbor_is_none_without_active_block() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        ..Default::default()
    };
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), -1);
}
