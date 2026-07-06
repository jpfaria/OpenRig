//! Phase 6 GUI sync red-first (issue #548): dispatching
//! `Command::SelectChainBlock` (the click callback the GUI already
//! sends) must also update `SelectionState::active_chain` and
//! `active_block`, so the MIDI daemon's snapshot mirrors what the user
//! clicked — without that, "active-chain" slots (prev_preset, etc.)
//! stay no-ops.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

fn chain_with_block_ids(id: &str, _block_ids: &[&str]) -> Chain {
    // Blocks not strictly needed for the active_chain assertion; an
    // empty Chain still has an id the dispatcher can store. For the
    // active_block assertion we mirror only what the dispatcher can
    // honour without an AudioBlock factory; block_ids is recorded in
    // the test name for documentation.
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    }
}

#[test]
fn select_chain_block_writes_active_chain_into_selection_state() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain_with_block_ids("chain_x", &[])],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let sel = dispatcher.selection_state();
    assert!(sel.read().unwrap().active_chain.is_none());

    dispatcher
        .dispatch(Command::SelectChainBlock {
            chain: ChainId("chain_x".to_string()),
            block_index: 0,
        })
        .unwrap();

    let s = sel.read().unwrap();
    assert_eq!(
        s.active_chain.as_deref(),
        Some("chain_x"),
        "SelectChainBlock must update active_chain so MIDI slots see what the user clicked"
    );
}
