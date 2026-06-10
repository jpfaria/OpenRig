//! #436 — per-input scene bank: dynamic scene count, per-scene chain
//! volume, add/remove scene. Split out of `rig_tests.rs` to keep each
//! test file under the 600-line Rust cap (one concern per file).

use super::*;
use crate::block::InputEntry;
use crate::chain::ChainInputMode;
use domain::ids::DeviceId;
use std::collections::BTreeMap;

fn source(device: &str, channels: Vec<usize>) -> InputEntry {
    InputEntry {
        device_id: DeviceId(device.into()),
        mode: ChainInputMode::Mono,
        channels,
    }
}

fn input(bank: &[(usize, &str)], active: usize) -> RigInput {
    RigInput {
        label: None,
        sources: vec![source("scarlett", vec![0])],
        bank: bank.iter().map(|(i, n)| (*i, n.to_string())).collect(),
        active_preset: active,
        active_scene: 1,
        routing: vec![],
        instrument: "electric_guitar".to_string(),
    }
}

fn project_with(inputs: Vec<(&str, RigInput)>, presets: &[&str]) -> RigProject {
    RigProject {
        name: Some("Studio".into()),
        inputs: inputs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        presets: presets
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    RigPreset {
                        id: String::new(),
                        name: None,
                        blocks: vec![],
                        scene_params: vec![],
                        scenes: BTreeMap::new(),
                        volume: 100.0,
                    },
                )
            })
            .collect(),
        midi: None,
    }
}

// #436 — scenes are added incrementally (a preset normally has just
// scene 1; the user adds 2, 3…). It makes no sense to force 8.
#[test]
fn scene_count_is_one_with_no_scenes_else_highest_index() {
    let mut pr = RigPreset::from_legacy_blocks(vec![], 100.0);
    assert_eq!(pr.scene_count(), 1, "implicit single Default scene");
    pr.scenes.insert(2, RigScene::default());
    assert_eq!(pr.scene_count(), 2);
    pr.scenes.insert(5, RigScene::default());
    assert_eq!(pr.scene_count(), 5, "highest defined index");
}

// #436 — chain volume is PER SCENE. A scene with no override inherits
// the preset volume; an override wins only for that scene.
#[test]
fn scene_volume_overrides_preset_only_for_that_scene() {
    let mut pr = RigPreset::from_legacy_blocks(vec![], 80.0);
    assert_eq!(pr.scene_volume(1), 80.0, "no override → preset volume");
    pr.scenes.insert(
        2,
        RigScene {
            volume: Some(100.0),
            ..RigScene::default()
        },
    );
    assert_eq!(pr.scene_volume(2), 100.0, "scene 2 override");
    assert_eq!(pr.scene_volume(1), 80.0, "scene 1 still preset volume");
}

#[test]
fn add_scene_to_input_appends_next_index_snapshots_active_and_activates() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    // Author a per-scene volume on the active scene (1) so we can prove
    // the new scene starts as an INDEPENDENT snapshot of it.
    p.write_back_chain_volume("input-1", 70.0);

    let idx = p.add_scene_to_input("input-1").expect("scene added");
    assert_eq!(idx, 2, "next scene index");
    assert_eq!(p.inputs["input-1"].active_scene, 2, "new scene is active");
    let pr = p.presets.get("p").unwrap();
    assert_eq!(pr.scene_count(), 2);
    assert_eq!(pr.scene_volume(2), 70.0, "snapshot of scene 1 at add time");

    // Editing scene 2 must not bleed into scene 1 (independent snapshot).
    p.write_back_chain_volume("input-1", 100.0);
    let pr = p.presets.get("p").unwrap();
    assert_eq!(pr.scene_volume(2), 100.0, "scene 2 edited");
    assert_eq!(pr.scene_volume(1), 70.0, "scene 1 KEEPS its own volume");
}

#[test]
fn add_scene_caps_at_eight() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    for expected in 2..=8 {
        assert_eq!(p.add_scene_to_input("input-1"), Some(expected));
    }
    assert_eq!(p.add_scene_to_input("input-1"), None, "8 is the max");
    assert_eq!(p.inputs["input-1"].active_scene, 8, "stays on last");
}

// User repro (#436): on a preset with 2 scenes, "add preset" kept 2
// scenes — a brand-new preset must start with a single (Default) scene.
#[test]
fn add_preset_starts_with_one_scene_even_if_source_has_many() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.add_scene_to_input("input-1"); // "p" now has 2 scenes
    assert_eq!(p.presets["p"].scene_count(), 2, "source preset has 2");

    let slot = p.add_preset_to_input("input-1").expect("preset added");
    let new_name = p.inputs["input-1"].bank[&slot].clone();
    assert_eq!(
        p.presets[&new_name].scene_count(),
        1,
        "a new preset starts fresh with a single scene"
    );
    assert_eq!(
        p.inputs["input-1"].active_scene, 1,
        "and the active scene resets to 1"
    );
}

#[test]
fn add_scene_unknown_input_is_none() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    assert_eq!(p.add_scene_to_input("nope"), None);
}

// #436 — scenes are removed like a stack (pop the last one), mirroring
// how they are added. The single remaining scene can't be removed.
#[test]
fn remove_last_scene_pops_and_clamps_active() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.add_scene_to_input("input-1"); // -> scene 2 (active)
    p.add_scene_to_input("input-1"); // -> scene 3 (active)
    p.write_back_chain_volume("input-1", 55.0); // scene 3 = 55
    p.inputs.get_mut("input-1").unwrap().active_scene = 2;
    p.write_back_chain_volume("input-1", 70.0); // scene 2 = 70
    p.inputs.get_mut("input-1").unwrap().active_scene = 3;

    // Removing while on the last scene drops it and falls back.
    let now = p.remove_last_scene_from_input("input-1").expect("removed");
    assert_eq!(now, 2, "active falls back to the new last scene");
    let pr = p.presets.get("p").unwrap();
    assert_eq!(pr.scene_count(), 2);
    assert_eq!(pr.scene_volume(2), 70.0, "scene 2 snapshot untouched");

    p.remove_last_scene_from_input("input-1").expect("removed");
    assert_eq!(p.presets.get("p").unwrap().scene_count(), 1);
    assert_eq!(
        p.remove_last_scene_from_input("input-1"),
        None,
        "can't remove the only remaining scene"
    );
}

#[test]
fn remove_scene_unknown_input_is_none() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    assert_eq!(p.remove_last_scene_from_input("nope"), None);
}

/// Red-first regression: the user reports "não consigo salvar
/// parâmetros na scene". The capture path
/// (`write_back_processing_blocks`) must persist an edited float
/// parameter into the **active** scene only, mark the path in
/// `scene_params`, and leave every other scene untouched.
#[test]
fn write_back_processing_blocks_persists_edited_param_into_active_scene() {
    use crate::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use crate::param::ParameterSet;
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;

    // Preset has one effect block with `drive=50.0`. Scenes don't
    // exist yet (single implicit Default scene 1).
    let block_id = "preamp:1".to_string();
    let mut base_params = ParameterSet::default();
    base_params.insert("drive", ParameterValue::Float(50.0));
    let base_block = AudioBlock {
        id: BlockId(block_id.clone()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: base_params,
        }),
    };
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.presets.get_mut("p").unwrap().blocks = vec![base_block.clone()];

    // Add scene 2 (becomes active).
    p.add_scene_to_input("input-1");

    // The projection feeds back an edited block on scene 2 with
    // `drive=80.0`. That's what the GUI/MCP would deliver to the
    // capture path.
    let mut edited_params = ParameterSet::default();
    edited_params.insert("drive", ParameterValue::Float(80.0));
    let edited = AudioBlock {
        id: BlockId(block_id.clone()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: edited_params,
        }),
    };
    p.write_back_processing_blocks("input-1", vec![edited]);

    let preset = p.presets.get("p").expect("preset present");
    let key = format!("{block_id}.drive");
    assert_eq!(
        preset
            .scenes
            .get(&2)
            .and_then(|s| s.params.get(&key).copied()),
        Some(80.0),
        "scene 2 must carry the edited override (got scenes={:?})",
        preset.scenes
    );
    assert!(
        preset.scene_params.iter().any(|k| k == &key),
        "the edited path must be marked as a scene-param (got {:?})",
        preset.scene_params
    );
    assert!(
        preset
            .scenes
            .get(&1)
            .map(|s| s.params.is_empty())
            .unwrap_or(true),
        "scene 1 must be untouched"
    );
}

/// Red-first regression: editing the same param across two scenes
/// must keep two distinct overrides; switching back to scene 1 and
/// re-projecting must not bleed the scene-2 value over scene 1.
#[test]
fn scene_params_remain_independent_across_scenes() {
    use crate::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use crate::param::ParameterSet;
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;

    let block_id = "preamp:1".to_string();
    let mut base_params = ParameterSet::default();
    base_params.insert("drive", ParameterValue::Float(50.0));
    let base_block = AudioBlock {
        id: BlockId(block_id.clone()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: base_params,
        }),
    };
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.presets.get_mut("p").unwrap().blocks = vec![base_block.clone()];

    // Scene 1 edit: drive=70.
    let mut params_s1 = ParameterSet::default();
    params_s1.insert("drive", ParameterValue::Float(70.0));
    let edit_s1 = AudioBlock {
        id: BlockId(block_id.clone()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: params_s1,
        }),
    };
    p.write_back_processing_blocks("input-1", vec![edit_s1]);

    // Switch to scene 2 (after adding it).
    p.add_scene_to_input("input-1"); // active_scene becomes 2

    // Scene 2 edit: drive=95.
    let mut params_s2 = ParameterSet::default();
    params_s2.insert("drive", ParameterValue::Float(95.0));
    let edit_s2 = AudioBlock {
        id: BlockId(block_id.clone()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: params_s2,
        }),
    };
    p.write_back_processing_blocks("input-1", vec![edit_s2]);

    let preset = p.presets.get("p").expect("preset present");
    let key = format!("{block_id}.drive");
    let s1_val = preset
        .scenes
        .get(&1)
        .and_then(|s| s.params.get(&key).copied());
    let s2_val = preset
        .scenes
        .get(&2)
        .and_then(|s| s.params.get(&key).copied());
    assert_eq!(s1_val, Some(70.0), "scene 1 keeps its own override");
    assert_eq!(s2_val, Some(95.0), "scene 2 keeps its own override");
}

/// Issue #690 — red-first: the user enables the noise gate on a NAM
/// block (a **Bool** param), saves, reopens, and it reverts. The f32
/// scene-diff cannot carry a bool, so the capture path silently dropped
/// every non-float param edit. The toggle must survive the
/// `write_back_processing_blocks` → `apply_scene` round-trip.
#[test]
fn write_back_persists_bool_param_edit_issue_690() {
    use crate::block::{AudioBlock, AudioBlockKind, NamBlock};
    use crate::param::ParameterSet;
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;

    let block_id = "nam:1".to_string();
    let mut base_params = ParameterSet::default();
    base_params.insert("noise_gate.enabled", ParameterValue::Bool(false));
    base_params.insert("noise_gate.threshold_db", ParameterValue::Float(-50.0));
    let base_block = AudioBlock {
        id: BlockId(block_id.clone()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: "nam_ts9".into(),
            params: base_params,
        }),
    };
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.presets.get_mut("p").unwrap().blocks = vec![base_block.clone()];

    // The user flips the gate ON; the projected chain feeds the edit back.
    let mut edited = base_block.clone();
    if let AudioBlockKind::Nam(ref mut nam) = edited.kind {
        nam.params
            .insert("noise_gate.enabled", ParameterValue::Bool(true));
    }
    p.write_back_processing_blocks("input-1", vec![edited]);

    let preset = p.presets.get("p").expect("preset present");
    let projected = preset.apply_scene(1);
    let gate = projected
        .iter()
        .find(|b| b.id.0 == block_id)
        .and_then(|b| match &b.kind {
            AudioBlockKind::Nam(nam) => nam.params.get_bool("noise_gate.enabled"),
            _ => None,
        });
    assert_eq!(
        gate,
        Some(true),
        "BUG #690: a Bool param edit must survive capture + re-projection \
         (preset blocks={:?})",
        preset.blocks
    );
}

#[test]
fn write_back_chain_volume_is_per_active_scene_only() {
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.add_scene_to_input("input-1"); // now on scene 2
    p.write_back_chain_volume("input-1", 100.0); // scene 2 = 100
    p.inputs.get_mut("input-1").unwrap().active_scene = 1;
    p.write_back_chain_volume("input-1", 50.0); // scene 1 = 50

    let pr = p.presets.get("p").unwrap();
    assert_eq!(pr.scene_volume(1), 50.0);
    assert_eq!(
        pr.scene_volume(2),
        100.0,
        "scene 2 untouched by scene 1 edit"
    );
    // Back at the preset default clears the override (no stale snapshot).
    p.write_back_chain_volume("input-1", 100.0);
    let pr = p.presets.get("p").unwrap();
    assert_eq!(
        pr.scenes.get(&1).and_then(|s| s.volume),
        None,
        "value == preset volume clears the per-scene override"
    );
}
