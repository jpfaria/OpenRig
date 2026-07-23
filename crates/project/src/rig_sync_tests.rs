//! Issue #502: prove `sync_synthetic_into_rig` captures the
//! user-defined chain order so save+reload preserves a reorder.
use super::*;
use crate::chain::Chain;
use crate::rig::{RigInput, RigPreset};
use domain::ids::ChainId;
use std::collections::BTreeMap;

fn rig_with_inputs(names: &[&str]) -> RigProject {
    let mut inputs = BTreeMap::new();
    let mut presets = BTreeMap::new();
    for name in names {
        inputs.insert(
            (*name).to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, format!("{name}_preset"))]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".to_string(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
            },
        );
        presets.insert(
            format!("{name}_preset"),
            RigPreset {
                id: String::new(),
                name: None,
                volume: 100.0,
                blocks: Vec::new(),
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

fn project_with_chain_ids(ids: &[&str]) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: ids
            .iter()
            .map(|id| Chain {
                id: ChainId((*id).into()),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: 100.0,
                io_binding_ids: Vec::new(),
                blocks: Vec::new(),
                di_output: None,
                loopers: vec![],
            })
            .collect(),
        midi: None,
    }
}

#[test]
fn sync_captures_reordered_chains_in_chain_order() {
    // Inputs "a" and "b" — alphabetical order is ["a", "b"]. After
    // MoveChainUp on "b", project.chains = ["rig:b", "rig:a"].
    let mut rig = rig_with_inputs(&["a", "b"]);
    let proj = project_with_chain_ids(&["rig:b", "rig:a"]);

    sync_synthetic_into_rig(&mut rig, &proj);

    assert_eq!(rig.chain_order, vec!["b".to_string(), "a".to_string()]);
}

#[test]
fn sync_leaves_chain_order_empty_when_alphabetical() {
    // Order matches the BTreeMap iteration — no need to write
    // chain_order. Keeps legacy `.openrig` files lean.
    let mut rig = rig_with_inputs(&["a", "b", "c"]);
    let proj = project_with_chain_ids(&["rig:a", "rig:b", "rig:c"]);

    sync_synthetic_into_rig(&mut rig, &proj);

    assert!(rig.chain_order.is_empty());
}

#[test]
fn sync_drops_stale_chain_order_entries_not_in_inputs() {
    let mut rig = rig_with_inputs(&["a", "b"]);
    // Project somehow still references "c" — that input was already
    // removed from rig.inputs.
    let proj = project_with_chain_ids(&["rig:c", "rig:b", "rig:a"]);

    sync_synthetic_into_rig(&mut rig, &proj);

    assert_eq!(
        rig.chain_order,
        vec!["b".to_string(), "a".to_string()],
        "stale name must not leak into chain_order"
    );
}

#[test]
fn sync_clears_chain_order_when_back_to_alphabetical() {
    // The rig was previously reordered to ["b", "a"]. The user
    // reorders it back to alphabetical → chain_order must reset to
    // empty so the YAML stays clean.
    let mut rig = rig_with_inputs(&["a", "b"]);
    rig.chain_order = vec!["b".to_string(), "a".to_string()];
    let proj = project_with_chain_ids(&["rig:a", "rig:b"]);

    sync_synthetic_into_rig(&mut rig, &proj);

    assert!(rig.chain_order.is_empty());
}

#[test]
fn sync_ignores_non_rig_chain_ids() {
    // A legacy `Project` may still hold non-prefixed chain ids.
    let mut rig = rig_with_inputs(&["a"]);
    let proj = project_with_chain_ids(&["legacy_chain", "rig:a"]);

    sync_synthetic_into_rig(&mut rig, &proj);

    // Only "a" was projected from the rig; the projected list of
    // rig-prefixed chains equals the alphabetical order ⇒ empty.
    assert!(rig.chain_order.is_empty());
}
