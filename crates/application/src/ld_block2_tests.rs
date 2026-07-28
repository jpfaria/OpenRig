//! PickFile / Remove/Move/Add/Replace block tests (issue #792 split from local_dispatcher_tests.rs).
//! Shared imports + helpers come from super::tests (pub(super)).

use super::local_dispatcher_tests::*;

// ── PickBlockParameterFile tests ──────────────────────────────────────────────

#[test]
fn pick_block_parameter_file_writes_path_and_emits_event() {
    let block = make_core_block_with_string_param("blk_0", "ir_path", "/old/path.wav");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    use std::path::PathBuf;
    let result = dispatcher.dispatch(Command::Block(BlockCommand::PickBlockParameterFile {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "ir_path".to_string(),
        file: PathBuf::from("/new/file.wav"),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockParameterChanged { chain, block, path }
            if chain.0 == "chain_0" && block.0 == "blk_0" && path == "ir_path"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("ir_path"),
        Some("/new/file.wav"),
        "ir_path must be updated to new file path"
    );
}

#[test]
fn pick_block_parameter_file_non_existent_block_returns_err() {
    use std::path::PathBuf;
    let block = make_core_block_with_string_param("blk_0", "ir_path", "/old/path.wav");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::PickBlockParameterFile {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        path: "ir_path".to_string(),
        file: PathBuf::from("/new/file.wav"),
    }));

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("ir_path"),
        Some("/old/path.wav"),
        "ir_path must not be mutated when block is not found"
    );
}

#[test]
fn pick_block_parameter_file_non_existent_path_returns_err() {
    use std::path::PathBuf;
    let block = make_core_block_with_string_param("blk_0", "ir_path", "/old/path.wav");
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::PickBlockParameterFile {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        path: "no_such_param".to_string(),
        file: PathBuf::from("/new/file.wav"),
    }));

    assert!(result.is_err(), "expected Err for missing path, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no_such_param"),
        "error message must mention the missing path, got: {err_msg}"
    );
    let proj = project.borrow();
    let AudioBlockKind::Core(ref core) = proj.chains[0].blocks[0].kind else {
        panic!("expected CoreBlock");
    };
    assert_eq!(
        core.params.get_string("ir_path"),
        Some("/old/path.wav"),
        "ir_path must not be mutated when path is not found"
    );
}

// ── RemoveBlock tests ─────────────────────────────────────────────────────────

fn make_project_two_blocks(chain_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                make_core_block("blk_0", true),
                make_core_block("blk_1", true),
            ],
            di_output: None,
        }],
        midi: None,
    }))
}

#[test]
fn remove_block_removes_block_and_emits_event() {
    let project = make_project_two_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::RemoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockRemoved { chain, block }
            if chain.0 == "chain_0" && block.0 == "blk_0"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    assert_eq!(
        proj.chains[0].blocks.len(),
        1,
        "chain must have 1 block after remove"
    );
    assert_eq!(
        proj.chains[0].blocks[0].id.0, "blk_1",
        "remaining block must be blk_1"
    );
}

#[test]
fn remove_block_non_existent_block_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::RemoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
    }));

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when block is not found"
    );
}

#[test]
fn remove_block_non_existent_chain_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::RemoveBlock {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_0".to_string()),
    }));

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when chain is not found"
    );
}

// ── MoveBlock tests ───────────────────────────────────────────────────────────

fn make_project_three_blocks(chain_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                make_core_block("blk_0", true),
                make_core_block("blk_1", true),
                make_core_block("blk_2", true),
            ],
            di_output: None,
        }],
        midi: None,
    }))
}

#[test]
fn move_block_reorders_blocks_and_emits_event() {
    // Move blk_2 to position 0 → order becomes [blk_2, blk_0, blk_1]
    let project = make_project_three_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::MoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_2".to_string()),
        new_position: 0,
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Event::ChainReloaded { chain } if chain.0 == "chain_0"),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["blk_2", "blk_0", "blk_1"],
        "blocks must be reordered"
    );
}

#[test]
fn move_block_past_end_clamps_to_end() {
    // Move blk_0 to position 999 → it should end up at the end: [blk_1, blk_2, blk_0]
    let project = make_project_three_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::MoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        new_position: 999,
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["blk_1", "blk_2", "blk_0"],
        "block must be clamped to end"
    );
}

#[test]
fn move_block_non_existent_block_returns_err() {
    let project = make_project_three_blocks("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::MoveBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        new_position: 0,
    }));

    assert!(result.is_err(), "expected Err for missing block, got Ok");
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains[0]
        .blocks
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["blk_0", "blk_1", "blk_2"],
        "order must not change when block not found"
    );
}

// ── AddBlock tests ────────────────────────────────────────────────────────────

#[test]
fn add_block_inserts_block_and_emits_event() {
    // chain_0 has one block; add a gain block at position 0
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::AddBlock {
        chain: ChainId("chain_0".to_string()),
        kind: "gain".to_string(),
        model_id: "fuzz_ge".to_string(),
        position: 0,
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Event::BlockAdded { chain, block: _ } if chain.0 == "chain_0"),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    assert_eq!(
        proj.chains[0].blocks.len(),
        2,
        "chain must have 2 blocks after add"
    );
    // The newly added block is at position 0
    let new_block = &proj.chains[0].blocks[0];
    assert_ne!(new_block.id.0, "blk_0", "inserted block must have a new id");
    assert!(new_block.enabled, "new block must be enabled by default");
}

#[test]
fn add_block_past_end_clamps_to_end() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::AddBlock {
        chain: ChainId("chain_0".to_string()),
        kind: "gain".to_string(),
        model_id: "fuzz_ge".to_string(),
        position: 999,
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    assert_eq!(
        proj.chains[0].blocks.len(),
        2,
        "chain must have 2 blocks after add"
    );
    // The original block is still at position 0; new block is at the end
    assert_eq!(
        proj.chains[0].blocks[0].id.0, "blk_0",
        "original block stays first"
    );
}

#[test]
fn add_block_non_existent_chain_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::AddBlock {
        chain: ChainId("chain_MISSING".to_string()),
        kind: "gain".to_string(),
        model_id: "fuzz_ge".to_string(),
        position: 0,
    }));

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when chain not found"
    );
}

#[test]
fn add_block_unknown_model_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::AddBlock {
        chain: ChainId("chain_0".to_string()),
        kind: "gain".to_string(),
        model_id: "no_such_model".to_string(),
        position: 0,
    }));

    assert!(result.is_err(), "expected Err for unknown model, got Ok");
    assert_eq!(
        project.borrow().chains[0].blocks.len(),
        1,
        "block list must not change when model is unknown"
    );
}

// ── ReplaceBlockModel tests ───────────────────────────────────────────────────

#[test]
fn replace_block_model_swaps_kind_preserves_id_enabled_and_emits_event() {
    let project = make_project("chain_0", make_core_block("blk_0", false));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        model_id: "fuzz_ge".to_string(),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(
            &events[0],
            Event::BlockReplaced { chain, block }
            if chain.0 == "chain_0" && block.0 == "blk_0"
        ),
        "unexpected event: {:?}",
        events[0]
    );
    let proj = project.borrow();
    let block = &proj.chains[0].blocks[0];
    // id and enabled must be preserved
    assert_eq!(
        block.id.0, "blk_0",
        "block id must be preserved after model swap"
    );
    assert!(
        !block.enabled,
        "block enabled state must be preserved after model swap"
    );
    // kind must have changed — it should now be a gain/fuzz_ge block
    assert!(
        matches!(&block.kind, AudioBlockKind::Core(cb) if cb.model == "fuzz_ge"),
        "block kind must be fuzz_ge after replace, got {:?}",
        block.kind.label()
    );
}

#[test]
fn replace_block_model_non_existent_block_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_MISSING".to_string()),
        model_id: "fuzz_ge".to_string(),
    }));

    assert!(result.is_err(), "expected Err for missing block, got Ok");
}

#[test]
fn replace_block_model_unknown_model_returns_err() {
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        model_id: "no_such_model_xyz".to_string(),
    }));

    assert!(result.is_err(), "expected Err for unknown model, got Ok");
    // Kind must not be mutated
    let proj = project.borrow();
    assert!(
        matches!(&proj.chains[0].blocks[0].kind, AudioBlockKind::Core(cb) if cb.model == "test_model"),
        "block kind must not change on error"
    );
}

// Issue #537 native-cab swap positive contract lives in
// `tests/issue_537_replace_block_model_native_cab.rs` (sibling to the
// disk-package fixture test). Kept out of this file because the file is
// already past the 600-line cap.
