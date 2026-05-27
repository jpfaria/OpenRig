//! Phase 6 GUI sync red-first (issue #548): the existing Set*/Toggle*
//! commands the GUI buttons dispatch must keep `SelectionState`'s
//! snapshot fields in lockstep so the corresponding MIDI toggle slots
//! see the current state and flip it correctly on the next press.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

fn project_with_chain(id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            blocks: vec![],
        }],
        midi: None,
    }))
}

#[test]
fn set_tuner_enabled_mirrors_into_selection_state() {
    let dispatcher = LocalDispatcher::new(project_with_chain("g"));
    let sel = dispatcher.selection_state();
    assert!(!sel.read().unwrap().tuner_enabled);

    dispatcher
        .dispatch(Command::SetTunerEnabled { enabled: true })
        .unwrap();
    assert!(sel.read().unwrap().tuner_enabled);

    dispatcher
        .dispatch(Command::SetTunerEnabled { enabled: false })
        .unwrap();
    assert!(!sel.read().unwrap().tuner_enabled);
}

#[test]
fn set_spectrum_enabled_mirrors_into_selection_state() {
    let dispatcher = LocalDispatcher::new(project_with_chain("g"));
    let sel = dispatcher.selection_state();
    assert!(!sel.read().unwrap().spectrum_enabled);

    dispatcher
        .dispatch(Command::SetSpectrumEnabled { enabled: true })
        .unwrap();
    assert!(sel.read().unwrap().spectrum_enabled);
}

#[test]
fn set_output_muted_mirrors_into_selection_state() {
    let dispatcher = LocalDispatcher::new(project_with_chain("g"));
    let sel = dispatcher.selection_state();
    assert!(!sel.read().unwrap().output_muted);

    dispatcher
        .dispatch(Command::SetOutputMuted { muted: true })
        .unwrap();
    assert!(sel.read().unwrap().output_muted);
}

#[test]
fn toggle_chain_enabled_mirrors_active_chain_enabled() {
    let dispatcher = LocalDispatcher::new(project_with_chain("g"));
    let sel = dispatcher.selection_state();

    // Mark chain "g" active first (so the snapshot is meaningful).
    {
        let mut s = sel.write().unwrap();
        s.active_chain = Some("g".to_string());
        s.active_chain_enabled = true; // matches the chain's default enabled=true
    }

    dispatcher
        .dispatch(Command::ToggleChainEnabled {
            chain: ChainId("g".to_string()),
        })
        .unwrap();

    // After toggle the chain's enabled flipped to false → snapshot must follow.
    assert!(
        !sel.read().unwrap().active_chain_enabled,
        "ToggleChainEnabled on the active chain must mirror into the snapshot"
    );
}
