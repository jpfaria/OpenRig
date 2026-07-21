//! Chain CRUD / toggle / io-binding / select / add tests (issue #792 split from local_dispatcher_tests.rs).
//! Shared imports + helpers come from super::tests (pub(super)).

use super::local_dispatcher_tests::*;

// ── Chain-level test helpers ──────────────────────────────────────────────────

/// Build a chain with an InputBlock on device `dev_id`, channel `ch`.
pub(super) fn make_chain_with_input(chain_id: &str, _dev_id: &str, _ch: usize, enabled: bool) -> Chain {
    Chain {
        id: ChainId(chain_id.to_string()),
        description: Some(chain_id.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("input:0".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                io: String::new(),
                endpoint: String::new(),
            }),
        }],
        di_output: None,
    }
}

/// Build a minimal chain with no blocks.
pub(super) fn make_empty_chain(chain_id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(chain_id.to_string()),
        description: Some(chain_id.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    }
}

/// Project with two chains: chain_0 (enabled, dev_a ch 0) and chain_1 (disabled, dev_a ch 0).
fn make_project_two_chains() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            make_chain_with_input("chain_0", "dev_a", 0, true),
            make_chain_with_input("chain_1", "dev_a", 0, false),
        ],
        midi: None,
    }))
}

/// Project with three chains in order: chain_a, chain_b, chain_c.
pub(super) fn make_project_three_chains() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![
            make_empty_chain("chain_a", false),
            make_empty_chain("chain_b", false),
            make_empty_chain("chain_c", false),
        ],
        midi: None,
    }))
}

// ── RemoveChain tests ─────────────────────────────────────────────────────────

#[test]
fn remove_chain_removes_chain_and_emits_event() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveChain {
        chain: ChainId("chain_0".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    // Should emit ChainRemoved + ProjectMutated.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainRemoved { chain } if chain.0 == "chain_0")),
        "expected ChainRemoved event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    let proj = project.borrow();
    assert_eq!(proj.chains.len(), 1, "one chain must remain");
    assert_eq!(proj.chains[0].id.0, "chain_1", "chain_1 must remain");
}

#[test]
fn remove_chain_non_existent_returns_err_no_mutation() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::RemoveChain {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert_eq!(
        project.borrow().chains.len(),
        2,
        "chain list must not change when chain not found"
    );
}

// ── MoveChainUp tests ─────────────────────────────────────────────────────────

#[test]
fn move_chain_up_reorders_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Move chain_b (index 1) up → should become index 0.
    let result = dispatcher.dispatch(Command::MoveChainUp {
        chain: ChainId("chain_b".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainMoved { chain, new_position: 0 }
            if chain.0 == "chain_b"
        )),
        "expected ChainMoved{{chain_b, 0}}, got {:?}",
        events
    );
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(ids, vec!["chain_b", "chain_a", "chain_c"]);
}

#[test]
fn move_chain_up_at_index_zero_returns_ok_no_op() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // chain_a is at index 0 — already at top, should be a no-op.
    let result = dispatcher.dispatch(Command::MoveChainUp {
        chain: ChainId("chain_a".to_string()),
    });

    assert!(result.is_ok(), "no-op should return Ok");
    let events = result.unwrap();
    assert!(events.is_empty(), "no-op must produce no events");
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(
        ids,
        vec!["chain_a", "chain_b", "chain_c"],
        "order must not change"
    );
}

#[test]
fn move_chain_up_non_existent_returns_err() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveChainUp {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

// ── MoveChainDown tests ───────────────────────────────────────────────────────

#[test]
fn move_chain_down_reorders_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Move chain_b (index 1) down → should become index 2.
    let result = dispatcher.dispatch(Command::MoveChainDown {
        chain: ChainId("chain_b".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainMoved { chain, new_position: 2 }
            if chain.0 == "chain_b"
        )),
        "expected ChainMoved{{chain_b, 2}}, got {:?}",
        events
    );
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(ids, vec!["chain_a", "chain_c", "chain_b"]);
}

#[test]
fn move_chain_down_at_last_index_returns_ok_no_op() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // chain_c is at the last index — should be a no-op.
    let result = dispatcher.dispatch(Command::MoveChainDown {
        chain: ChainId("chain_c".to_string()),
    });

    assert!(result.is_ok(), "no-op should return Ok");
    let events = result.unwrap();
    assert!(events.is_empty(), "no-op must produce no events");
    let proj = project.borrow();
    let ids: Vec<&str> = proj.chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(
        ids,
        vec!["chain_a", "chain_b", "chain_c"],
        "order must not change"
    );
}

#[test]
fn move_chain_down_non_existent_returns_err() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::MoveChainDown {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

// ── ToggleChainEnabled tests ──────────────────────────────────────────────────

#[test]
fn toggle_chain_enabled_refuses_chain_without_io_binding() {
    // #716: a chain with no I/O (no io_binding_ids and no bound input) routes
    // nothing — enabling it produces no sound and invalidates the project. The
    // dispatcher must refuse to enable it and leave it disabled.
    use project::chain::Chain;
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain_noio".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_noio".to_string()),
    });

    assert!(
        result.is_err(),
        "enabling a chain with no I/O binding must be rejected, got {result:?}"
    );
    assert!(
        !project.borrow().chains[0].enabled,
        "the chain must stay disabled"
    );
}

#[test]
fn toggle_chain_enabled_enables_disabled_chain() {
    // A clean enable: chain_1 carries an I/O binding (model A: `has_io()` is
    // true when `io_binding_ids` is non-empty), so the dispatcher enables it.
    let mut chain_1 = make_chain_with_input("chain_1", "dev_b", 0, false);
    chain_1.io_binding_ids = vec!["io".to_string()];
    let project_no_conflict = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![make_chain_with_input("chain_0", "dev_a", 0, true), chain_1],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project_no_conflict));

    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_1".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainEnabledChanged { chain, enabled: true }
            if chain.0 == "chain_1"
        )),
        "expected ChainEnabledChanged{{chain_1, true}}, got {:?}",
        events
    );
    assert!(
        project_no_conflict.borrow().chains[1].enabled,
        "chain_1 must be enabled after toggle"
    );
}

// ── SetChainIoBindings (#716) ──────────────────────────────────────────────────

#[test]
fn set_chain_io_bindings_updates_selection_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![make_chain_with_input("chain_0", "dev_a", 0, false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainIoBindings {
        chain: ChainId("chain_0".to_string()),
        binding_ids: vec!["xyz".to_string(), "abc".to_string()],
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    assert_eq!(
        project.borrow().chains[0].io_binding_ids,
        vec!["xyz".to_string(), "abc".to_string()],
        "the chain's selected bindings must be stored"
    );
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainIoBindingsChanged { chain, binding_ids }
            if chain.0 == "chain_0" && binding_ids == &vec!["xyz".to_string(), "abc".to_string()]
        )),
        "expected ChainIoBindingsChanged, got {:?}",
        events
    );
}

#[test]
fn set_chain_io_bindings_replaces_previous_selection() {
    let mut chain = make_chain_with_input("chain_0", "dev_a", 0, false);
    chain.io_binding_ids = vec!["old".to_string()];
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: ChainId("chain_0".to_string()),
            binding_ids: vec!["new".to_string()],
        })
        .expect("dispatch ok");

    assert_eq!(
        project.borrow().chains[0].io_binding_ids,
        vec!["new".to_string()],
        "selection is replaced wholesale (checklist sends the full set)"
    );
}

#[test]
fn toggle_chain_enabled_disables_enabled_chain() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Toggle chain_0 (currently enabled) → should disable it.
    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_0".to_string()),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainEnabledChanged { chain, enabled: false }
            if chain.0 == "chain_0"
        )),
        "expected ChainEnabledChanged{{chain_0, false}}, got {:?}",
        events
    );
    assert!(
        !project.borrow().chains[0].enabled,
        "chain_0 must be disabled after toggle"
    );
}

// #716 (model A): `toggle_chain_enabled_conflict_returns_err` was removed.
// It asserted the per-block cross-chain channel-conflict check (two chains on
// the same device/channel) — device endpoints no longer live on the chain, so
// that check moved to the per-machine binding registry at the activation layer
// (a separate task). There is no model-A equivalent at this dispatcher layer.

#[test]
fn toggle_chain_enabled_non_existent_returns_err() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ToggleChainEnabled {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

// ── SelectActiveChain tests (issue #591) ───────────────────────────────────────

#[test]
fn select_active_chain_sets_active_chain_clears_block_and_snapshots_enabled() {
    // chain_0 (enabled), chain_1 (disabled). The footswitch slot
    // `toggle_active_chain_enabled` resolves against `active_chain`, so
    // selecting a chain on the Chains screen must set it as the active one
    // — otherwise the footswitch stays frozen on whatever was selected last
    // (the #591 bug: it always targeted `rig:input-3`).
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Seed selection on chain_0 with a block, as if the user had drilled in.
    {
        let sel = dispatcher.selection_state();
        let mut s = sel.write().unwrap();
        s.active_chain = Some("chain_0".to_string());
        s.active_block = Some("input:0".to_string());
        s.active_chain_enabled = true;
    }

    let result = dispatcher.dispatch(Command::SelectActiveChain {
        chain: ChainId("chain_1".to_string()),
    });
    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);

    let sel = dispatcher.selection_state();
    let s = sel.read().unwrap();
    assert_eq!(
        s.active_chain.as_deref(),
        Some("chain_1"),
        "selecting a chain on screen must become the active chain the footswitch toggles"
    );
    assert!(
        s.active_block.is_none(),
        "changing the active chain must clear the stale active block (block lives in one chain)"
    );
    assert!(
        !s.active_chain_enabled,
        "active_chain_enabled must snapshot the newly-selected chain (chain_1 is disabled)"
    );
}

#[test]
fn select_active_chain_non_existent_returns_err() {
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SelectActiveChain {
        chain: ChainId("chain_MISSING".to_string()),
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn set_compact_view_enabled_emits_event_so_the_gui_can_react() {
    // #591: a footswitch bound to `toggle_compact_view` dispatches
    // SetCompactViewEnabled. The handler only flipped a SelectionState
    // snapshot and returned no events, so the MIDI drain had nothing to
    // act on and the compact view never opened ("isso não faz nada").
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SetCompactViewEnabled { enabled: true })
        .expect("dispatch ok");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CompactViewEnabledChanged { enabled: true })),
        "expected CompactViewEnabledChanged{{true}}, got {events:?}"
    );
}

// ── AddChain tests ────────────────────────────────────────────────────────────

#[test]
fn add_chain_appends_chain_and_emits_event() {
    let project = make_project_three_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_chain = make_empty_chain("chain_new", false);
    let new_id = new_chain.id.clone();

    let result = dispatcher.dispatch(Command::AddChain { chain: new_chain });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainAdded { chain } if *chain == new_id)),
        "expected ChainAdded event, got {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    assert_eq!(
        project.borrow().chains.len(),
        4,
        "project must have 4 chains after add"
    );
    assert_eq!(
        project.borrow().chains.last().unwrap().id,
        new_id,
        "new chain must be last"
    );
}

// #716 (model A): `add_chain_enabled_true_with_conflict_returns_err` was
// removed. It asserted the per-block cross-chain channel-conflict check on
// `AddChain` (a new enabled chain on the same device/channel as an existing
// enabled chain). Device endpoints no longer live on the chain, so that check
// moved to the per-machine binding registry at the activation layer (a separate
// task). There is no model-A equivalent at this dispatcher layer.

#[test]
fn add_chain_enabled_false_no_conflict_check() {
    // chain_0 is enabled on dev_a ch 0. Add a new chain (enabled=false) on same channel → ok.
    let project = make_project_two_chains();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_chain = make_chain_with_input("chain_new", "dev_a", 0, false); // enabled=false

    let result = dispatcher.dispatch(Command::AddChain { chain: new_chain });

    assert!(
        result.is_ok(),
        "disabled chain add must succeed even with same channel"
    );
    assert_eq!(project.borrow().chains.len(), 3);
}

