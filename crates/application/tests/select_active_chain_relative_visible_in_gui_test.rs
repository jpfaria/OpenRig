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

    // The legacy GUI looks up `selected_block(&chain)` to decide which
    // chain to highlight — even when there is no block selected, the
    // entry's presence is the signal.
    assert_eq!(
        dispatcher.selected_block(&ChainId("chain_a".to_string())),
        Some(0),
        "the active chain must appear in the legacy selection map so the existing highlight kicks in"
    );

    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "must emit ProjectMutated so the GUI drain loop re-renders; got {events:?}"
    );
}
