use super::{rig_nav_rows, RigNavRow};
use engine::rig_runtime::{rig_to_legacy_project, switch_and_project_input};
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
fn switch_then_nav_reflects_new_active_preset_and_scene() {
    // The exact round-trip the GUI wiring performs, pure (no AppWindow):
    // switch_and_project_input mutates the rig, the chains are
    // re-projected, and rig_nav_rows must report the new active state.
    let mut r = rig(); // input-1: bank {1 clean, 2 drive, 3 lead}, active 2, scene 4
    let before = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(before[0].active_index, 1, "active preset 2 → index 1");
    assert_eq!(before[0].scene, 4);

    let chain =
        switch_and_project_input(&mut r, "input-1", Some(3), Some(7)).expect("rebuilt chain");
    assert_eq!(chain.id.0, "rig:input-1");

    let after = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(after[0].active_index, 2, "active preset 3 → index 2");
    assert_eq!(after[0].scene, 7);
}

#[test]
fn non_rig_chain_yields_empty_row() {
    let r = rig();
    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    proj.chains[0].id = domain::ids::ChainId("legacy-thing".into());
    let rows = rig_nav_rows(&r, &proj);
    assert_eq!(rows[0], RigNavRow::default(), "no selectors for non-rig");
}

#[test]
fn sync_synthetic_into_rig_writes_edited_blocks_back_to_active_preset() {
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(80.0));
    let blk = AudioBlock {
        id: BlockId("vol".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params,
        }),
    };
    let mut r = rig();
    r.presets.get_mut("clean").unwrap().blocks = vec![blk.clone()];
    // active preset of input-1 is slot 2 ("drive") in rig(); point it at clean.
    r.inputs.get_mut("input-1").unwrap().active_preset = 1;

    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    // User edits the param on the projected synthetic chain.
    for c in proj.chains.iter_mut().filter(|c| c.id.0 == "rig:input-1") {
        for b in c.blocks.iter_mut() {
            if let AudioBlockKind::Core(core) = &mut b.kind {
                core.params.insert("volume", ParameterValue::Float(90.0));
            }
        }
    }

    let sc = r.inputs["input-1"].active_scene;
    super::sync_synthetic_into_rig(&mut r, &proj);

    // Snapshot: the edit lands in the active scene, not the template.
    let preset = r.presets.get("clean").unwrap();
    let active = match &preset.apply_scene(sc)[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    assert_eq!(
        active,
        Some(90.0),
        "synthetic edit written into active scene"
    );
    let base = match &preset.blocks[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    assert_eq!(base, Some(80.0), "factory template untouched");
}
