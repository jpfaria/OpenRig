//! Phase 3b red-first test (issue #548): real handlers for the 3 MIDI
//! selection commands.
//!
//! `LocalDispatcher` exposes its `SelectionState` so:
//! - `SelectActiveChainRelative { delta }` cycles through `project.chains`
//!   (wraps), clears `active_block` on chain change.
//! - `SetCompactViewEnabled { enabled }` stores the flag in
//!   `SelectionState::compact_view_enabled`.
//!
//! `SelectActiveBlockRelative` is exercised separately through the
//! existing local_dispatcher tests (block construction needs the
//! project crate's internal factories).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

fn chain_named(id: &str) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
    }
}

fn project_with(chains: Vec<Chain>) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    }))
}

#[test]
fn dispatcher_exposes_selection_state_handle() {
    let project = project_with(vec![chain_named("chain_0")]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let sel = dispatcher.selection_state();
    assert!(sel.read().unwrap().active_chain.is_none());
    assert!(sel.read().unwrap().active_block.is_none());
    assert!(!sel.read().unwrap().compact_view_enabled);
}

#[test]
fn select_active_chain_relative_advances_and_wraps() {
    let project = project_with(vec![
        chain_named("chain_a"),
        chain_named("chain_b"),
        chain_named("chain_c"),
    ]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.selection_state().write().unwrap().active_chain = Some("chain_a".to_string());

    dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    let sel = dispatcher.selection_state();
    assert_eq!(sel.read().unwrap().active_chain.as_deref(), Some("chain_b"));

    dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    let sel = dispatcher.selection_state();
    assert_eq!(sel.read().unwrap().active_chain.as_deref(), Some("chain_c"));

    dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    let sel = dispatcher.selection_state();
    assert_eq!(
        sel.read().unwrap().active_chain.as_deref(),
        Some("chain_a"),
        "must wrap"
    );

    dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: -1 })
        .unwrap();
    let sel = dispatcher.selection_state();
    assert_eq!(
        sel.read().unwrap().active_chain.as_deref(),
        Some("chain_c"),
        "negative delta wraps backwards"
    );
}

#[test]
fn select_active_chain_relative_clears_active_block_on_change() {
    let project = project_with(vec![chain_named("chain_a"), chain_named("chain_b")]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let sel_handle = dispatcher.selection_state();
    {
        let mut s = sel_handle.write().unwrap();
        s.active_chain = Some("chain_a".to_string());
        s.active_block = Some("blk_held_over".to_string());
    }

    dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();

    let sel = sel_handle.read().unwrap();
    assert_eq!(sel.active_chain.as_deref(), Some("chain_b"));
    assert!(
        sel.active_block.is_none(),
        "block must clear when chain changes (block belongs to chain)"
    );
}

#[test]
fn select_active_chain_relative_seeds_first_when_none_active() {
    let project = project_with(vec![chain_named("chain_a"), chain_named("chain_b")]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    // no active chain yet (fresh load)
    dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();
    let sel = dispatcher.selection_state();
    assert_eq!(
        sel.read().unwrap().active_chain.as_deref(),
        Some("chain_a"),
        "first chain becomes active on first nav"
    );
}

#[test]
fn set_compact_view_enabled_stores_the_flag() {
    let project = project_with(vec![chain_named("chain_a")]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    assert!(
        !dispatcher
            .selection_state()
            .read()
            .unwrap()
            .compact_view_enabled
    );

    dispatcher
        .dispatch(Command::SetCompactViewEnabled { enabled: true })
        .unwrap();
    assert!(
        dispatcher
            .selection_state()
            .read()
            .unwrap()
            .compact_view_enabled
    );

    dispatcher
        .dispatch(Command::SetCompactViewEnabled { enabled: false })
        .unwrap();
    assert!(
        !dispatcher
            .selection_state()
            .read()
            .unwrap()
            .compact_view_enabled
    );
}
