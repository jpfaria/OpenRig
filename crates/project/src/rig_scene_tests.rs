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
