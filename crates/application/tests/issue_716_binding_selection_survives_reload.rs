//! #716 RED — selecting an I/O binding in the chain editor checklist must
//! survive reopening the project.
//!
//! User repro: open project TESTE → chain 1 → "configure chain" → the bindings
//! come back UNCHECKED even though a binding was selected + saved before.
//!
//! Root cause this test pins: the editor checklist persists the selection as
//! `Chain.io_binding_ids`, but the app stores the project as a RIG, and the rig
//! model has no slot for it — `rig_to_legacy_project` forces
//! `io_binding_ids = Vec::new()` (rig_sync.rs:144). So the selection is dropped
//! on the rig round-trip the app performs on every reopen.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::rig::{RigInput, RigPreset, RigProject};

/// A rig with one input chain ("rig:in"), mirroring chain_io_binding_tests.
fn rig_with_chain() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: None,
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

#[test]
fn binding_selection_survives_rig_reopen() {
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));

    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    let chain_id = ChainId("rig:in".to_string());

    // User checks a binding in the editor → SetChainIoBindings persists it.
    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: chain_id.clone(),
            binding_ids: vec!["main".to_string()],
        })
        .expect("SetChainIoBindings must succeed");

    // Reopen: the app rebuilds the project from the persisted rig.
    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    assert_eq!(
        chain.io_binding_ids,
        vec!["main".to_string()],
        "the selected binding must survive reopening the project; got io_binding_ids = {:?}. \
         The rig model has no io_binding_ids slot, so rig_to_legacy_project drops it and the \
         editor checklist reopens UNCHECKED.",
        chain.io_binding_ids
    );
}

#[test]
fn two_binding_selection_survives_rig_reopen() {
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: ChainId("rig:in".to_string()),
            binding_ids: vec!["a".to_string(), "b".to_string()],
        })
        .expect("SetChainIoBindings must succeed");

    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    assert_eq!(
        chain.io_binding_ids,
        vec!["a".to_string(), "b".to_string()],
        "both selected bindings (in order) must survive reopen; got {:?}",
        chain.io_binding_ids
    );
}

#[test]
fn binding_selection_via_savechain_survives_rig_reopen() {
    // The GUI editor persists the checklist via Command::SaveChain (upsert),
    // NOT SetChainIoBindings. This proves the real GUI path persists too.
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    // Build the edited chain exactly like chain_from_draft would: same id,
    // with the binding selected.
    let mut edited = project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("seed chain")
        .clone();
    edited.io_binding_ids = vec!["main".to_string()];

    dispatcher
        .dispatch(Command::SaveChain { chain: edited })
        .expect("SaveChain must succeed");

    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    assert_eq!(
        chain.io_binding_ids,
        vec!["main".to_string()],
        "binding selected + saved via the GUI's SaveChain must survive reopen; got {:?}",
        chain.io_binding_ids
    );
}
