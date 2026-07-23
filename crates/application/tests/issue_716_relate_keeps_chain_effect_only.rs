//! #716 RED (#3) — relating an I/O binding must NOT make Input/Output blocks
//! appear in the chain the user sees/saves.
//!
//! The binding is a REFERENCE (`io_binding_ids`), resolved into transient I/O
//! blocks only at runtime (engine `io_routing` / `runtime_io_graph`). The
//! persisted/displayed chain must stay effect-only. Today, relating + reopening
//! surfaces synthesized I/O blocks in the chain strip (the "monster").
//!
//! These tests drive the real persistence flow (command → rig → reopen) and
//! assert the reopened chain carries no Input/Output blocks. Faithful: they
//! fail because the reopened chain DOES carry I/O blocks, and go green only
//! when the bound chain is represented as a reference (no materialized blocks
//! in the saved model) — without touching the engine's runtime materialization.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::block::AudioBlockKind;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};

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

fn relate_and_reopen(binding_ids: Vec<String>) -> Project {
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
            binding_ids,
        })
        .expect("SetChainIoBindings must succeed");

    let reopened = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    reopened
}

fn io_block_count(project: &Project) -> usize {
    project
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .filter(|b| matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .count()
}

#[test]
fn relating_one_binding_keeps_reopened_chain_effect_only() {
    let reopened = relate_and_reopen(vec!["main".to_string()]);
    let n = io_block_count(&reopened);
    assert_eq!(
        n, 0,
        "after relating one binding and reopening, the chain must carry NO Input/Output \
         blocks (I/O is a reference, materialized only at runtime); got {n} I/O block(s) in \
         the saved chain — the user sees them as the 'monster' in the chain strip"
    );
}

#[test]
fn relating_two_bindings_keeps_reopened_chain_effect_only() {
    let reopened = relate_and_reopen(vec!["a".to_string(), "b".to_string()]);
    let n = io_block_count(&reopened);
    assert_eq!(
        n, 0,
        "relating two bindings must not stack per-endpoint I/O blocks into the saved chain; \
         got {n} I/O block(s) — this is the multi-binding 'monster' the user reported"
    );
}
