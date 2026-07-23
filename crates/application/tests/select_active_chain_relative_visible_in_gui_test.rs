//! Phase 6 GUI feedback red-first (issue #548): when MIDI dispatches
//! `SelectActiveChainRelative`, the GUI must learn that the active
//! chain changed so it can render the highlight. Two contracts:
//!
//! - the dispatcher's legacy per-chain block-selection map (the one
//!   `SelectChainBlock` populates) seeds an entry for the new active
//!   chain, so the existing GUI "selected chain" indicator picks it up.
//! - the handler emits `Event::ProjectMutated` so the GUI's drain loop
//!   re-renders.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

fn chain(id: &str) -> Chain {
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
fn midi_chain_step_seeds_legacy_selection_map_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("chain_a"), chain("chain_b")],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SelectActiveChainRelative { delta: 1 })
        .unwrap();

    // The dispatcher's SelectionState is the single source the GUI, MIDI,
    // and MCP (`QueryKind::Selection`) all read to highlight the active chain.
    assert_eq!(
        dispatcher
            .selection_state()
            .read()
            .unwrap()
            .active_chain
            .as_deref(),
        Some("chain_a"),
        "SelectActiveChainRelative must record the active chain in SelectionState"
    );

    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "must emit ProjectMutated so the GUI drain loop re-renders; got {events:?}"
    );
}
