//! Issue #591 regression: selecting a chain on the Chains screen
//! (`Command::SelectActiveChain`) must make the footswitch slot
//! `toggle_active_chain_enabled` target THAT chain.
//!
//! The MIDI daemon mirrors `SelectionState` from the dispatcher, so the
//! slot resolved from the live selection must follow the on-screen
//! selection instead of a stale/frozen chain (the bug: the footswitch was
//! frozen on `rig:input-3` regardless of what the user selected).

use std::cell::RefCell;
use std::rc::Rc;

use adapter_midi::slots::{slot_to_command, IncomingMessage};
use application::command::{ChainId, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use project::chain::Chain;
use project::project::Project;

fn disabled_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn footswitch_toggle_targets_the_chain_just_selected() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![disabled_chain("rig:input-1"), disabled_chain("rig:input-3")],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(project);

    // User taps chain rig:input-1 on the Chains screen.
    dispatcher
        .dispatch(Command::SelectActiveChain {
            chain: ChainId("rig:input-1".to_string()),
        })
        .unwrap();

    // The footswitch slot resolves against the live (mirrored) SelectionState.
    let sel = dispatcher.selection_state();
    let guard = sel.read().unwrap();
    let cmd = slot_to_command(
        "toggle_active_chain_enabled",
        &IncomingMessage::ProgramChange {
            channel: 1,
            program: 1,
        },
        &guard,
    )
    .expect("an active chain is selected, so the slot must resolve");

    match cmd {
        Command::ToggleChainEnabled { chain } => assert_eq!(
            chain,
            ChainId("rig:input-1".to_string()),
            "the footswitch must toggle the chain the user just selected, not a stale one"
        ),
        other => panic!("expected ToggleChainEnabled{{rig:input-1}}, got {other:?}"),
    }
}
