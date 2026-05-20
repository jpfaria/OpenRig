//! Issue #502 coverage for `rig_to_chains` honouring `RigProject.chain_order`.
//! Kept out of `rig_runtime_tests.rs` because that file already exceeds the
//! 600-line cap; mod is wired from `rig_runtime.rs` next to the existing
//! `rig_runtime_tests` include.

use std::collections::BTreeMap;

use project::block::AudioBlock;
use project::rig::{RigInput, RigPreset, RigProject};

use super::rig_to_chains;

fn input_with_preset(preset_key: &str) -> RigInput {
    RigInput {
        label: None,
        sources: Vec::new(),
        bank: BTreeMap::from([(1, preset_key.to_string())]),
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
    }
}

fn rig_with_inputs(names: &[&str]) -> RigProject {
    let mut inputs = BTreeMap::new();
    let mut presets = BTreeMap::new();
    for name in names {
        let preset_key = format!("{name}_preset");
        inputs.insert((*name).to_string(), input_with_preset(&preset_key));
        presets.insert(
            preset_key,
            RigPreset {
                id: String::new(),
                name: None,
                volume: 100.0,
                blocks: Vec::<AudioBlock>::new(),
                scenes: BTreeMap::new(),
                scene_params: Vec::new(),
            },
        );
    }
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

fn chain_input_names(rig: &RigProject) -> Vec<String> {
    rig_to_chains(rig)
        .into_iter()
        .filter_map(|c| c.id.0.strip_prefix("rig:").map(String::from))
        .collect()
}

#[test]
fn rig_to_chains_falls_back_to_alphabetical_when_chain_order_empty() {
    // Back-compat: a `.openrig` file without `chain-order` keeps its
    // historical alphabetical projection.
    let rig = rig_with_inputs(&["b", "a"]);
    assert_eq!(
        chain_input_names(&rig),
        vec!["a".to_string(), "b".to_string()],
        "empty chain_order ⇒ BTreeMap iteration order"
    );
}

#[test]
fn rig_to_chains_honours_chain_order_when_set() {
    let mut rig = rig_with_inputs(&["a", "b"]);
    rig.chain_order = vec!["b".to_string(), "a".to_string()];
    assert_eq!(
        chain_input_names(&rig),
        vec!["b".to_string(), "a".to_string()],
        "chain_order must drive the projection order"
    );
}

#[test]
fn rig_to_chains_appends_missing_inputs_alphabetically_after_chain_order() {
    // The user reorders ["a", "b"] then later adds input "c". The
    // saved chain_order still says ["b", "a"]; "c" must still surface.
    let mut rig = rig_with_inputs(&["a", "b", "c"]);
    rig.chain_order = vec!["b".to_string(), "a".to_string()];
    assert_eq!(
        chain_input_names(&rig),
        vec!["b".to_string(), "a".to_string(), "c".to_string()]
    );
}

#[test]
fn rig_to_chains_ignores_stale_chain_order_entries() {
    let mut rig = rig_with_inputs(&["a", "b"]);
    rig.chain_order = vec!["ghost".to_string(), "b".to_string(), "a".to_string()];
    assert_eq!(
        chain_input_names(&rig),
        vec!["b".to_string(), "a".to_string()],
        "names absent from rig.inputs must not produce phantom chains"
    );
}

#[test]
fn rig_to_chains_drops_duplicate_chain_order_entries() {
    let mut rig = rig_with_inputs(&["a", "b"]);
    rig.chain_order = vec!["a".to_string(), "a".to_string(), "b".to_string()];
    assert_eq!(
        chain_input_names(&rig),
        vec!["a".to_string(), "b".to_string()]
    );
}
