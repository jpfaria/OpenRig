use super::{rig_nav_rows, RigNavRow};
use engine::rig_runtime::rig_to_legacy_project;
use project::block::InputEntry;
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};
use std::collections::{BTreeMap, BTreeSet};

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    for n in ["clean", "drive", "lead"] {
        presets.insert(
            n.to_string(),
            RigPreset {
                blocks: vec![],
                scene_params: vec![],
                scenes: BTreeMap::new(),
                volume: 100.0,
            },
        );
    }
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: domain::ids::DeviceId("d".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank: BTreeMap::from([
                (1, "clean".to_string()),
                (2, "drive".to_string()),
                (3, "lead".to_string()),
            ]),
            active_preset: 2,
            active_scene: 4,
            routing: vec![],
        },
    );
    RigProject {
        name: Some("p".into()),
        inputs,
        outputs: BTreeMap::new(),
        presets,
    }
}

#[test]
fn nav_row_exposes_bank_active_and_scene_aligned_to_chains() {
    let r = rig();
    let proj = rig_to_legacy_project(&r, &BTreeSet::new());
    let rows = rig_nav_rows(&r, &proj);

    assert_eq!(rows.len(), proj.chains.len(), "one row per chain, aligned");
    let row = &rows[0];
    assert_eq!(row.input, "input-1");
    assert_eq!(row.preset_slots, vec![1, 2, 3]);
    assert_eq!(row.preset_labels, vec!["clean", "drive", "lead"]);
    assert_eq!(row.active_index, 1, "active_preset 2 → index 1");
    assert_eq!(row.scene, 4);
}

#[test]
fn non_rig_chain_yields_empty_row() {
    let r = rig();
    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    proj.chains[0].id = domain::ids::ChainId("legacy-thing".into());
    let rows = rig_nav_rows(&r, &proj);
    assert_eq!(rows[0], RigNavRow::default(), "no selectors for non-rig");
}
