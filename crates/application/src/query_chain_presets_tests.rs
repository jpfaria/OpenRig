//! Red-first (#554) tests for `Query::ListChainPresets`.

use std::collections::BTreeMap;

use domain::ids::ChainId;
use project::rig::{RigInput, RigPreset, RigProject};

use crate::query::list_chain_presets;

fn rig_with_input(input_name: &str, bank: Vec<(usize, &str)>, active_preset: usize) -> RigProject {
    let mut inputs = BTreeMap::new();
    let mut presets = BTreeMap::new();
    let mut input = RigInput {
        label: None,
        sources: Vec::new(),
        bank: BTreeMap::new(),
        active_preset,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".to_string(),
        io: String::new(),
        endpoint: String::new(),
    };
    for (idx, name) in bank {
        input.bank.insert(idx, name.to_string());
        presets
            .entry(name.to_string())
            .or_insert_with(|| RigPreset {
                id: name.to_string(),
                name: Some(name.to_string()),
                blocks: Vec::new(),
                scene_params: Vec::new(),
                scenes: BTreeMap::new(),
                volume: 100.0,
            });
    }
    inputs.insert(input_name.to_string(), input);
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn list_chain_presets_returns_bank_in_slot_order() {
    let rig = rig_with_input(
        "input-1",
        vec![(2, "Lead"), (1, "Clean strum"), (3, "Clocks — Coldplay")],
        2,
    );
    let chain = ChainId("rig:input-1".to_string());
    let json = list_chain_presets(&rig, &chain).expect("ok");
    // Slots ordered by index: 1, 2, 3 — inspect the `slots:[...]`
    // portion explicitly so the assertion isn't fooled by the
    // active_preset field appearing earlier in the JSON.
    let slots_start = json.find("\"slots\":").expect("slots field");
    let slots = &json[slots_start..];
    let pos_clean = slots.find("Clean strum").expect("Clean strum in slots");
    let pos_lead = slots.find("Lead").expect("Lead in slots");
    let pos_clocks = slots.find("Clocks — Coldplay").expect("Clocks in slots");
    assert!(pos_clean < pos_lead, "slot 1 must come before slot 2");
    assert!(pos_lead < pos_clocks, "slot 2 must come before slot 3");
    assert!(
        json.contains("\"active_preset\":\"Lead\""),
        "active preset slot=2 resolves to 'Lead', got: {json}"
    );
}

#[test]
fn list_chain_presets_resolves_chain_id_to_input_name() {
    let rig = rig_with_input("guitar", vec![(1, "Solo")], 1);
    let chain = ChainId("rig:guitar".to_string());
    let json = list_chain_presets(&rig, &chain).expect("ok");
    assert!(json.contains("rig:guitar"), "chain id echoed: {json}");
    assert!(json.contains("Solo"));
}

#[test]
fn list_chain_presets_empty_bank_returns_empty_slots() {
    let mut rig = rig_with_input("input-1", vec![], 0);
    rig.inputs.get_mut("input-1").unwrap().active_preset = 0;
    let chain = ChainId("rig:input-1".to_string());
    let json = list_chain_presets(&rig, &chain).expect("empty bank is not an error");
    assert!(
        json.contains("\"slots\":[]"),
        "empty slots array, got: {json}"
    );
    assert!(
        json.contains("\"active_preset\":null"),
        "no active preset when bank empty, got: {json}"
    );
}

#[test]
fn list_chain_presets_unknown_chain_returns_error() {
    let rig = rig_with_input("input-1", vec![(1, "Clean")], 1);
    let chain = ChainId("rig:does-not-exist".to_string());
    let err = list_chain_presets(&rig, &chain).expect_err("unknown chain must err");
    assert!(
        err.contains("does-not-exist"),
        "error mentions the missing input, got: {err}"
    );
}

#[test]
fn list_chain_presets_rejects_non_rig_chain_id() {
    let rig = rig_with_input("input-1", vec![], 0);
    let chain = ChainId("standalone-chain".to_string());
    let err = list_chain_presets(&rig, &chain).expect_err("non-rig chain must err");
    assert!(
        err.contains("rig:"),
        "error mentions the rig: prefix requirement, got: {err}"
    );
}

#[test]
fn list_chain_presets_is_deterministic() {
    let rig = rig_with_input("input-1", vec![(3, "C"), (1, "A"), (2, "B")], 1);
    let chain = ChainId("rig:input-1".to_string());
    let a = list_chain_presets(&rig, &chain).unwrap();
    let b = list_chain_presets(&rig, &chain).unwrap();
    assert_eq!(a, b, "two reads must produce the same bytes");
}
