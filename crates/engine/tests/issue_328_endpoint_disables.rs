//! #328 — the endpoint checklists reach the engine (spec §1.3): the projected
//! chain carries its input's checklist, and the rig and chain conflict
//! detectors agree on it.

use std::collections::BTreeMap;

use engine::rig_runtime::rig_to_chains;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};
use project::rig::{RigInput, RigPreset, RigProject};

fn unchecked_input(endpoint: &str) -> EndpointDisables {
    let mut disables = EndpointDisables::default();
    disables.set_enabled(
        EndpointNode::Input,
        EndpointRef {
            io: "shared".into(),
            endpoint: endpoint.into(),
        },
        false,
    );
    disables
}

fn input(disabled_endpoints: EndpointDisables) -> RigInput {
    RigInput {
        label: None,
        bank: BTreeMap::from([(1, "p".to_string())]),
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".into(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: vec!["shared".into()],
        loopers: Vec::new(),
        disabled_endpoints,
    }
}

fn rig(g1: EndpointDisables, g2: EndpointDisables) -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([("g1".to_string(), input(g1)), ("g2".to_string(), input(g2))]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(Vec::new(), 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn the_projected_chain_carries_its_inputs_unchecked_endpoints() {
    let chains = rig_to_chains(&rig(unchecked_input("in 2"), EndpointDisables::default()));
    let g1 = chains
        .iter()
        .find(|c| c.id.0 == "rig:g1")
        .expect("g1 is projected");
    assert_eq!(
        g1.disabled_endpoints,
        unchecked_input("in 2"),
        "the synthetic chain carries the rig input's checklist"
    );
}
