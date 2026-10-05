//! #328 — a checklist edit made on the projected chain reaches the rig (the
//! save path), and a new chain saved into the rig keeps its checklist.

use std::collections::BTreeMap;

use domain::ids::ChainId;
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};
use project::migrate::migrate_legacy_project;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};
use project::rig_sync::sync_synthetic_into_rig;

fn unchecked() -> EndpointDisables {
    let mut disables = EndpointDisables::default();
    disables.set_enabled(
        &EndpointNode::Output,
        EndpointRef {
            io: "io".into(),
            endpoint: "out R".into(),
        },
        false,
    );
    disables
}

fn chain(id: &str, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        mix: Default::default(),
        id: ChainId(id.into()),
        description: Some("Guitar".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: Vec::new(),
        di_output: None,
        loopers: Vec::new(),
        disabled_endpoints,
    }
}

fn project_with(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains,
        midi: None,
    }
}

#[test]
fn a_checklist_edit_on_the_projected_chain_is_captured_into_the_rig_input() {
    let mut rig = RigProject {
        bpm: None,
        name: None,
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                mix: Default::default(),
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: vec!["io".into()],
                loopers: Vec::new(),
                disabled_endpoints: EndpointDisables::default(),
                di_output: None,
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(Vec::new(), 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    };
    sync_synthetic_into_rig(&mut rig, &project_with(vec![chain("rig:g", unchecked())]));
    assert_eq!(
        rig.inputs["g"].disabled_endpoints,
        unchecked(),
        "the save path captures the checklist into project.yaml"
    );
}

#[test]
fn a_new_chain_saved_into_the_rig_keeps_its_checklist() {
    let rig = migrate_legacy_project(&project_with(vec![chain("chain:new", unchecked())]));
    assert_eq!(rig.inputs["input-1"].disabled_endpoints, unchecked());
}
