//! Issue #717 Task 3 — RED-FIRST test for `Command::SetChainDiLoopOutput`.
//!
//! Dispatching `SetChainDiLoopOutput` must:
//! - persist `chain.di_output = Some(DiOutputRef { ... })` on the matching chain,
//! - emit `Event::ChainDiLoopOutputChanged { chain }`.
//!
//! A missing chain must return `Err`.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::{Chain, DiOutputRef};
use project::project::Project;

fn make_project(chain_id: &str) -> Rc<RefCell<Project>> {
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
            blocks: vec![],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    }))
}

/// Dispatching `SetChainDiLoopOutput` must persist `di_output` on the chain
/// and emit `Event::ChainDiLoopOutputChanged`.
#[test]
fn set_chain_di_loop_output_persists_and_emits_event() {
    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let output = DiOutputRef {
        binding_id: "binding_A".to_string(),
        endpoint: "out_L".to_string(),
    };

    let events = dispatcher
        .dispatch(Command::SetChainDiLoopOutput {
            chain: ChainId("chain_0".to_string()),
            output: output.clone(),
        })
        .expect("SetChainDiLoopOutput must succeed");

    // The event must have been emitted.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainDiLoopOutputChanged { chain }
            if chain.0 == "chain_0"
        )),
        "expected Event::ChainDiLoopOutputChanged for chain_0, got {events:?}"
    );

    // The project state must be updated.
    let proj = project.borrow();
    let chain = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "chain_0")
        .expect("chain must exist");

    assert_eq!(
        chain.di_output,
        Some(output),
        "chain.di_output must be Some(DiOutputRef {{ binding_id: \"binding_A\", endpoint: \"out_L\" }})"
    );
}

/// Dispatching `SetChainDiLoopOutput` for a non-existent chain must return `Err`.
#[test]
fn set_chain_di_loop_output_missing_chain_returns_err() {
    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainDiLoopOutput {
        chain: ChainId("chain_MISSING".to_string()),
        output: DiOutputRef {
            binding_id: "binding_A".to_string(),
            endpoint: "out_L".to_string(),
        },
    });

    assert!(result.is_err(), "missing chain must return Err");
}
