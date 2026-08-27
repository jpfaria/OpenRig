//! #826 RED — editing a chain in the chain editor must not erase the loops it
//! recorded.
//!
//! Owner repro: "reopened the app and the looper is EMPTY". Forensics on the
//! real project: `project.yaml` carries no `loopers:` key at all, while four
//! recorded wavs sit orphaned in `project.loops/`. So the loopers were not
//! merely unpointed — they were deleted from the model.
//!
//! The path: the chain editor's Save builds the chain with `loopers: vec![]`
//! (`chain_editor::chain_from_draft`, edit mode) and `SaveChain` upserts it
//! with `*existing = chain`, so renaming a chain or ticking an I/O binding
//! drops every looper. The next rig capture writes the empty list into the
//! rig, and the save serializes a project that has forgotten the loops.
//!
//! A looper is created and recorded through its OWN commands; `SaveChain`
//! carries the editor's fields and has no business speaking about loops —
//! exactly like `enabled`, which the handler already preserves by hand.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use application::command::{ChainCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use project::chain::LooperConfig;
use project::rig::{RigInput, RigPreset, RigProject};

/// A rig with one input chain ("rig:in") whose looper has audio on disk.
fn rig_with_a_recorded_loop() -> RigProject {
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
            loopers: vec![LooperConfig {
                audio_file: Some("rig-in-looper-1.wav".into()),
                ..LooperConfig::new(1)
            }],
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
fn renaming_a_chain_keeps_the_loops_it_recorded() {
    let rig = Rc::new(RefCell::new(rig_with_a_recorded_loop()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    // Exactly what `chain_from_draft` hands over in edit mode: the existing
    // chain with a new name and NO loopers.
    let mut edited = project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("seed chain")
        .clone();
    edited.description = Some("GUITARRA - TONES".into());
    edited.loopers = vec![];

    dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: edited }))
        .expect("SaveChain must succeed");

    let proj = project.borrow();
    let chain = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must still exist");
    assert_eq!(
        chain.loopers.len(),
        1,
        "renaming a chain must not delete the loopers it carries; got {:?}",
        chain.loopers
    );
    assert_eq!(
        chain.loopers[0].audio_file.as_deref(),
        Some("rig-in-looper-1.wav"),
        "the recorded loop's audio must still be pointed at after a chain edit"
    );
}

#[test]
fn the_edited_chain_still_carries_its_loops_into_the_rig() {
    // The second half of the loss: the rig capture copies the project's chain
    // loopers over the rig's, so an emptied chain erases them from what gets
    // written to disk.
    let rig = Rc::new(RefCell::new(rig_with_a_recorded_loop()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    let mut edited = project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("seed chain")
        .clone();
    edited.io_binding_ids = vec!["main".to_string()];
    edited.loopers = vec![];

    dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: edited }))
        .expect("SaveChain must succeed");
    project::rig_sync::sync_synthetic_into_rig(&mut rig.borrow_mut(), &project.borrow());

    let reopened = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reopened
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");
    assert_eq!(
        chain.loopers.len(),
        1,
        "the loop recorded before the chain edit must survive save + reopen; got {:?}",
        chain.loopers
    );
}
