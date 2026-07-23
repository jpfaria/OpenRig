//! ConfigureChain / SaveChain / endpoint tests (issue #792 split from local_dispatcher_tests.rs).
//! Shared imports + helpers come from super::tests (pub(super)).

use super::local_dispatcher_tests::*;

// ── ConfigureChain tests ──────────────────────────────────────────────────────

#[test]
fn configure_chain_updates_metadata_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Get chain_b's id, build an updated version.
    let chain_b_id = project.borrow().chains[1].id.clone();
    let mut updated = project.borrow().chains[1].clone();
    updated.description = Some("Updated Name".to_string());
    updated.instrument = "bass".to_string();

    let result = dispatcher.dispatch(Command::ConfigureChain { chain: updated });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainConfigured { chain } if *chain == chain_b_id)),
        "expected ChainConfigured event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    let proj = project.borrow();
    let chain_b = proj.chains.iter().find(|c| c.id == chain_b_id).unwrap();
    assert_eq!(chain_b.description.as_deref(), Some("Updated Name"));
    assert_eq!(chain_b.instrument, "bass");
}

#[test]
fn configure_chain_preserves_enabled_state() {
    // chain_a starts enabled=false. Even if the supplied Chain has enabled=true,
    // the dispatcher must preserve the original enabled state.
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let chain_a_id = project.borrow().chains[0].id.clone();
    let mut updated = project.borrow().chains[0].clone();
    updated.enabled = true; // caller tries to sneak in enabled=true via ConfigureChain

    let result = dispatcher.dispatch(Command::ConfigureChain { chain: updated });

    assert!(result.is_ok());
    let proj = project.borrow();
    let chain_a = proj.chains.iter().find(|c| c.id == chain_a_id).unwrap();
    assert!(
        !chain_a.enabled,
        "enabled state must be preserved (use ToggleChainEnabled to change it)"
    );
}

#[test]
fn configure_chain_non_existent_returns_err() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let mut ghost = make_empty_chain("chain_ghost", false);
    ghost.id = ChainId("chain_MISSING".to_string());

    let result = dispatcher.dispatch(Command::ConfigureChain { chain: ghost });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains.len(),
        3,
        "chain list must not change"
    );
}

// ── SaveChain tests ───────────────────────────────────────────────────────────

#[test]
fn save_chain_replaces_existing_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let chain_b_id = project.borrow().chains[1].id.clone();
    let mut updated = project.borrow().chains[1].clone();
    updated.description = Some("Chain B Updated".to_string());
    updated.instrument = "bass".to_string();

    let result = dispatcher.dispatch(Command::SaveChain { chain: updated });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainSaved { chain } if *chain == chain_b_id)),
        "expected ChainSaved event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated"
    );
    let proj = project.borrow();
    let chain_b = proj.chains.iter().find(|c| c.id == chain_b_id).unwrap();
    assert_eq!(chain_b.description.as_deref(), Some("Chain B Updated"));
    assert_eq!(chain_b.instrument, "bass");
    // enabled must be preserved
    assert!(!chain_b.enabled, "enabled must be preserved by SaveChain");
}

#[test]
fn save_chain_appends_when_id_not_found() {
    // SaveChain with an unknown id should append (create flow).
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_chain = make_empty_chain("chain_brand_new", false);

    let result = dispatcher.dispatch(Command::SaveChain { chain: new_chain });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    assert_eq!(
        project.borrow().chains.len(),
        4,
        "chain must be appended when id not found"
    );
    assert_eq!(
        project.borrow().chains.last().unwrap().id.0,
        "chain_brand_new"
    );
}

#[test]
fn save_chain_preserves_enabled_state() {
    // Even if caller sends enabled=true in the chain, SaveChain must keep the
    // existing enabled state (same rule as ConfigureChain).
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let chain_a_id = project.borrow().chains[0].id.clone();
    let mut updated = project.borrow().chains[0].clone();
    updated.enabled = true; // sneaking in enabled=true

    let result = dispatcher.dispatch(Command::SaveChain { chain: updated });

    assert!(result.is_ok());
    let proj = project.borrow();
    let chain_a = proj.chains.iter().find(|c| c.id == chain_a_id).unwrap();
    assert!(!chain_a.enabled, "SaveChain must preserve enabled state");
}

// ── SaveChainInputEndpoints tests ─────────────────────────────────────────────
//
// New semantics: SaveChainInputEndpoints { chain, block_index, io, endpoint }
// sets `block.io` and `block.endpoint` on the InputBlock at `block_index`.
// Returns Err when chain not found, block_index out-of-bounds, or the target
// block is not an InputBlock.

pub(super) fn make_project_with_input_chain() -> (Rc<RefCell<Project>>, ChainId) {
    let chain = make_chain_with_input("chain_io", "dev_x", 0, false);
    let chain_id = chain.id.clone();
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    }));
    (project, chain_id)
}

fn make_input_block(_dev_id: &str, _ch: usize) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

pub(super) fn make_output_block(_dev_id: &str, _ch: usize) -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

#[test]
fn save_chain_input_endpoints_sets_io_and_endpoint_and_emits_event() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // The chain built by make_project_with_input_chain has the input block at index 0.
    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        block_index: 0,
        io: "main".to_string(),
        endpoint: "Guitar In".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainInputEndpointsSaved { chain } if *chain == chain_id)),
        "expected ChainInputEndpointsSaved, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let input_block = chain
        .blocks
        .iter()
        .find(|b| matches!(&b.kind, AudioBlockKind::Input(_)))
        .expect("input block must exist");
    if let AudioBlockKind::Input(ib) = &input_block.kind {
        assert_eq!(ib.io, "main", "io must be set");
        assert_eq!(ib.endpoint, "Guitar In", "endpoint must be set");
    }
}

#[test]
fn save_chain_input_endpoints_out_of_bounds_returns_err() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        block_index: 999,
        io: "main".to_string(),
        endpoint: "Guitar In".to_string(),
    });

    assert!(
        result.is_err(),
        "expected Err for out-of-bounds block_index"
    );
}

#[test]
fn save_chain_input_endpoints_wrong_block_type_returns_err() {
    // Chain: [Input, core insert]
    let chain_id = ChainId("chain_type".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                make_input_block("dev_x", 0),
                make_core_block("blk_mid", true),
            ],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // block_index 1 is a core Insert block, not an InputBlock → must fail.
    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        block_index: 1,
        io: "main".to_string(),
        endpoint: "Guitar In".to_string(),
    });

    assert!(result.is_err(), "expected Err for wrong block type");
}

#[test]
fn save_chain_input_endpoints_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: ChainId("chain_MISSING".to_string()),
        block_index: 0,
        io: "main".to_string(),
        endpoint: "Guitar In".to_string(),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_input_endpoints_preserves_other_blocks() {
    // Chain: [Input(index 0), CoreA(index 1), CoreB(index 2), Output(index 3)]
    // Setting io/endpoint on index 0 must not disturb other blocks.
    let chain_id = ChainId("chain_order".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                make_input_block("dev_old", 0),
                make_core_block("blk_a", true),
                make_core_block("blk_b", true),
                make_output_block("dev_out", 1),
            ],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: chain_id.clone(),
        block_index: 0,
        io: "binding1".to_string(),
        endpoint: "ep1".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    // 4 blocks preserved; only io/endpoint on block 0 changed
    assert_eq!(chain.blocks.len(), 4);
    assert_eq!(chain.blocks[1].id, BlockId("blk_a".to_string()));
    assert_eq!(chain.blocks[2].id, BlockId("blk_b".to_string()));
    assert!(matches!(&chain.blocks[3].kind, AudioBlockKind::Output(_)));
    if let AudioBlockKind::Input(ib) = &chain.blocks[0].kind {
        assert_eq!(ib.io, "binding1");
        assert_eq!(ib.endpoint, "ep1");
    }
}

// ── SaveChainOutputEndpoints tests ────────────────────────────────────────────
//
// New semantics: SaveChainOutputEndpoints { chain, block_index, io, endpoint }
// sets `block.io` and `block.endpoint` on the OutputBlock at `block_index`.

pub(super) fn make_project_with_io_chain() -> (Rc<RefCell<Project>>, ChainId) {
    let chain_id = ChainId("chain_io_full".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![make_input_block("dev_a", 0), make_output_block("dev_b", 1)],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }));
    (project, chain_id)
}

#[test]
fn save_chain_output_endpoints_sets_io_and_endpoint_and_emits_event() {
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // io chain: [Input(idx 0), Output(idx 1)]
    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        block_index: 1,
        io: "main".to_string(),
        endpoint: "Monitor Out".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainOutputEndpointsSaved { chain } if *chain == chain_id
        )),
        "expected ChainOutputEndpointsSaved, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let output_block = chain
        .blocks
        .iter()
        .find(|b| matches!(&b.kind, AudioBlockKind::Output(_)))
        .expect("output block must exist");
    if let AudioBlockKind::Output(ob) = &output_block.kind {
        assert_eq!(ob.io, "main", "io must be set");
        assert_eq!(ob.endpoint, "Monitor Out", "endpoint must be set");
    }
}

#[test]
fn save_chain_output_endpoints_out_of_bounds_returns_err() {
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        block_index: 999,
        io: "main".to_string(),
        endpoint: "Monitor Out".to_string(),
    });

    assert!(
        result.is_err(),
        "expected Err for out-of-bounds block_index"
    );
}

#[test]
fn save_chain_output_endpoints_wrong_block_type_returns_err() {
    // io chain: [Input(0), Output(1)] — index 0 is Input, not Output.
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        block_index: 0, // This is an Input block
        io: "main".to_string(),
        endpoint: "Monitor Out".to_string(),
    });

    assert!(
        result.is_err(),
        "expected Err for wrong block type (Input ≠ Output)"
    );
}

#[test]
fn save_chain_output_endpoints_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId("chain_MISSING".to_string()),
        block_index: 0,
        io: "main".to_string(),
        endpoint: "Monitor Out".to_string(),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_output_endpoints_preserves_other_blocks() {
    // Chain: [Input(0), CoreA(1), CoreB(2), Output(3)]
    let chain_id = ChainId("chain_out_order".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                make_input_block("dev_in", 0),
                make_core_block("blk_a", true),
                make_core_block("blk_b", true),
                make_output_block("dev_out_old", 1),
            ],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: chain_id.clone(),
        block_index: 3,
        io: "binding2".to_string(),
        endpoint: "ep2".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    // 4 blocks preserved
    assert_eq!(chain.blocks.len(), 4);
    assert!(matches!(&chain.blocks[0].kind, AudioBlockKind::Input(_)));
    assert_eq!(chain.blocks[1].id, BlockId("blk_a".to_string()));
    assert_eq!(chain.blocks[2].id, BlockId("blk_b".to_string()));
    if let AudioBlockKind::Output(ob) = &chain.blocks[3].kind {
        assert_eq!(ob.io, "binding2");
        assert_eq!(ob.endpoint, "ep2");
    }
}

