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
        // Input, b0, b1, Output — the strip draws all four (model A, #716).
        blocks: vec![
            io_block("in", true),
            core_block("b0"),
            core_block("b1"),
            io_block("out", false),
        ],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
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
fn active_chain_and_block_marks_both_with_the_blocks_position() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b1".to_string()),
        ..Default::default()
    };
    // chain 0 is [in, b0, b1, out]; the strip draws all four (model A, #716),
    // so "b1" is chip 2.
    assert_eq!(active_highlight_indices(&project(), &sel), (0, 2));
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
fn neighbor_is_the_next_chip() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()), // chip 1
        ..Default::default()
    };
    // The toggle-neighbor command targets the raw-next block → b1, chip 2.
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), 2);
}

#[test]
fn neighbor_of_the_last_block_is_the_port_after_it() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b1".to_string()),
        ..Default::default()
    };
    // The raw-next block is the mid `Output` port, which the strip draws as
    // chip 3 — so it is markable.
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), 3);
}

#[test]
fn neighbor_is_none_without_active_block() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        ..Default::default()
    };
    assert_eq!(active_neighbor_block_ui_index(&project(), &sel), -1);
}

/// #328 (spec §5.2): the strip draws EVERY entry of `chain.blocks` (model A,
/// #716 — `project_chains_refresh.rs`), so a mid `Input` port sits at chip 0
/// and the block after it at chip 1. The old mapping skipped "the first
/// Input", so selecting the block after a port lit the port's chip instead.
#[test]
fn the_highlight_lands_on_the_chip_the_strip_draws_for_the_block() {
    let mut ported = chain("rig:input-1");
    ported.blocks = vec![
        io_block("port-in", true),
        core_block("amp"),
        io_block("port-out", false),
    ];
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![ported],
        midi: None,
    };
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("amp".to_string()),
        ..Default::default()
    };
    assert_eq!(active_highlight_indices(&project, &sel), (0, 1));
}

fn chain_of(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    let mut c = chain(id);
    c.blocks = blocks;
    c
}

fn highlight(blocks: Vec<AudioBlock>, active: &str) -> (i32, i32) {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain_of("rig:input-1", blocks)],
        midi: None,
    };
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some(active.to_string()),
        ..Default::default()
    };
    active_highlight_indices(&project, &sel)
}

#[test]
fn every_block_highlights_at_its_own_position() {
    // Was `real_block_index_to_ui_maps_effect_blocks_correctly`.
    let blocks = || {
        vec![
            io_block("in", true),
            core_block("comp"),
            core_block("pre"),
            core_block("dly"),
            io_block("out", false),
        ]
    };
    assert_eq!(highlight(blocks(), "comp"), (0, 1));
    assert_eq!(highlight(blocks(), "pre"), (0, 2));
    assert_eq!(highlight(blocks(), "dly"), (0, 3));
}

#[test]
fn a_port_block_is_highlightable() {
    // Was `real_block_index_to_ui_hidden_blocks_return_none`: ports are chips now.
    let blocks = || {
        vec![
            io_block("in", true),
            core_block("dly"),
            io_block("out", false),
        ]
    };
    assert_eq!(highlight(blocks(), "in"), (0, 0));
    assert_eq!(highlight(blocks(), "out"), (0, 2));
}

#[test]
fn a_block_that_is_not_in_the_chain_marks_no_chip() {
    // Was `real_block_index_to_ui_out_of_range_returns_none`.
    assert_eq!(
        highlight(vec![io_block("in", true), io_block("out", false)], "gone"),
        (0, -1)
    );
}

/// #328: the graph marks cards by block id; same rule as the strip's indices.
#[test]
fn the_graph_marks_the_selected_block_and_its_neighbor_by_id() {
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()),
        ..Default::default()
    };
    assert_eq!(
        graph_selection_ids(&project(), &sel),
        (0, "b0".to_string(), "b1".to_string())
    );
}

#[test]
fn nothing_selected_marks_no_card() {
    assert_eq!(
        graph_selection_ids(&project(), &SelectionState::default()),
        (-1, String::new(), String::new())
    );
}

/// The footswitch drain calls `sync_selection_markers`: the graph rows must
/// follow it like the strip does.
#[test]
fn syncing_the_markers_feeds_the_chain_graph_too() {
    use slint::Global;
    i_slint_backend_testing::init_no_event_loop();
    let window = crate::AppWindow::new().unwrap();
    let sel = SelectionState {
        active_chain: Some("rig:input-1".to_string()),
        active_block: Some("b0".to_string()),
        ..Default::default()
    };
    sync_selection_markers(&window, &project(), &sel);
    let bridge = crate::ChainGraphBridge::get(&window);
    assert_eq!(
        (
            bridge.get_selected_chain_index(),
            bridge.get_selected_block_id().to_string(),
            bridge.get_neighbor_block_id().to_string(),
        ),
        (0, "b0".to_string(), "b1".to_string())
    );
}
