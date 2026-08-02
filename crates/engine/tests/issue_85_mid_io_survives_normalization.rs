//! #85 — a mid-chain `Output` the user placed must SURVIVE a project load.
//!
//! #716 normalizes a binding-bound chain by dropping every `Input`/`Output`
//! block, because the bug it fixed was a LEGACY LEFTOVER: an old Output block on
//! the same binding the chain is already bound to, duplicating the binding's
//! output and starving the device.
//!
//! That blanket drop also deletes the mid port the user places on purpose (#85),
//! so the block vanishes on reload. The rule has to be narrowed to what it was
//! actually protecting against: an I/O block pointing at a binding the chain
//! ALREADY carries in `io_binding_ids` is the duplicate — one pointing anywhere
//! else is a real mid port and must stay.

use std::collections::{BTreeMap, BTreeSet};

use domain::ids::BlockId;
use engine::rig_runtime::rig_to_legacy_project;
use project::block::{AudioBlock, AudioBlockKind, OutputBlock};
use project::rig::{RigInput, RigPreset, RigProject};

fn output_block(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    }
}

/// A chain bound to `main`, carrying the given blocks.
fn rig_with(blocks: Vec<AudioBlock>) -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".to_string(),
        RigPreset::from_legacy_blocks(blocks, 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".to_string());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".to_string(),
        RigInput {
            label: None,
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: vec!["main".to_string()],
            loopers: Vec::new(),
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

fn output_blocks(rig: &RigProject) -> Vec<OutputBlock> {
    let project = rig_to_legacy_project(rig, &BTreeSet::new());
    project
        .chains
        .first()
        .expect("one chain")
        .blocks
        .iter()
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(o) => Some(o.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_mid_output_on_another_binding_survives_the_load() {
    let rig = rig_with(vec![output_block("rig:in:aux", "aux", "out0")]);
    let outputs = output_blocks(&rig);
    assert_eq!(
        outputs.len(),
        1,
        "#85: the mid Output the user placed points at the `aux` E/S, not at the \
         chain's own binding — it is not a legacy duplicate and must survive the \
         load, or the block vanishes every time the project is reopened"
    );
    assert_eq!(outputs[0].io, "aux");
    assert_eq!(outputs[0].endpoint, "out0");
}

#[test]
fn a_leftover_output_on_the_chains_own_binding_is_still_dropped() {
    let rig = rig_with(vec![output_block("rig:in:out", "main", "out0")]);
    assert!(
        output_blocks(&rig).is_empty(),
        "#716 must keep holding: an Output on the binding the chain is ALREADY \
         bound to duplicates the binding's own output and starves the device"
    );
}
