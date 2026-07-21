//! Rig preset write-back / structural-swap tests (issue #792 split from
//! rig_tests.rs). Shares input/project_with/core_block via super::tests.

use crate::block::AudioBlock;

use super::rig_tests::{core_block, input, project_with};
use super::*;

// #436 #1 — write-back: editing a rig chain's processing blocks must
// persist into the active preset and survive re-projection.
#[test]
fn write_back_processing_blocks_persists_edit_into_active_preset() {
    use domain::value_objects::ParameterValue;

    let mut vol = core_block("vol");
    if let AudioBlockKind::Core(c) = &mut vol.kind {
        c.params.insert("volume", ParameterValue::Float(80.0));
    }
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.inputs.get_mut("input-1").unwrap().active_scene = 2;
    p.presets.get_mut("p").unwrap().blocks = vec![vol.clone()];

    // User edits the param on the (re-projected) synthetic chain.
    let mut edited = vol.clone();
    if let AudioBlockKind::Core(c) = &mut edited.kind {
        c.params.insert("volume", ParameterValue::Float(90.0));
    }

    p.write_back_processing_blocks("input-1", vec![edited]);

    // Snapshot semantics: the edit is captured into the ACTIVE scene
    // (2), not baked into the factory template. apply_scene(2) reflects
    // it; the base template stays unchanged.
    let preset = p.presets.get("p").unwrap();
    let active = match &preset.apply_scene(2)[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    assert_eq!(active, Some(90.0), "edit visible on the scene it was made");
    let base = match &preset.blocks[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    assert_eq!(base, Some(80.0), "factory template untouched");
}

// #436 #1 — SNAPSHOT scenes: editing a param while on scene N must NOT
// leak into other scenes (user: changed volume on scene 2 → it changed
// in ALL scenes). Reproduces the bug; drives the snapshot fix.
#[test]
fn editing_on_scene_2_does_not_change_scene_1() {
    use domain::value_objects::ParameterValue;

    let mut vol = core_block("vol");
    if let AudioBlockKind::Core(c) = &mut vol.kind {
        c.params.insert("volume", ParameterValue::Float(80.0));
    }
    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.presets.get_mut("p").unwrap().blocks = vec![vol.clone()];

    // User is on scene 2 and sets volume to 100.
    p.inputs.get_mut("input-1").unwrap().active_scene = 2;
    let mut edited = vol.clone();
    if let AudioBlockKind::Core(c) = &mut edited.kind {
        c.params.insert("volume", ParameterValue::Float(100.0));
    }
    p.write_back_processing_blocks("input-1", vec![edited]);

    let vol_in = |blocks: &[AudioBlock]| match &blocks[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    let preset = p.presets.get("p").unwrap();
    assert_eq!(
        vol_in(&preset.apply_scene(2)),
        Some(100.0),
        "scene 2 = edited"
    );
    assert_eq!(
        vol_in(&preset.apply_scene(1)),
        Some(80.0),
        "scene 1 must KEEP its own value (snapshot, not shared base)"
    );
}

// #436 — "como adiciono um preset?" Adding a preset to an input's bank
// User-reported contract change: a brand-new preset must start
// FRESH. Cloning the currently active preset's blocks / volume was
// confusing — switching to the new slot looked identical to the
// previous one, so the "+" button felt broken. New shape: empty
// blocks, default volume (100.0), one default scene, becomes active.
#[test]
fn add_preset_to_input_creates_fresh_preset_with_no_blocks_or_volume_clone() {
    use crate::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use crate::param::ParameterSet;
    use domain::ids::BlockId;

    let mut p = project_with(
        vec![(
            "input-1",
            input(&[(1, "clean"), (2, "drive"), (3, "lead")], 2),
        )],
        &["clean", "drive", "lead"],
    );
    // Make the active preset ("drive") distinguishable: pump volume
    // and add a Core block so we can prove neither leaks into the
    // freshly-added preset.
    p.presets.get_mut("drive").unwrap().volume = 137.0;
    p.presets.get_mut("drive").unwrap().blocks.push(AudioBlock {
        id: BlockId("gain:1".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: ParameterSet::default(),
        }),
    });
    let presets_before = p.presets.len();

    let slot = p
        .add_preset_to_input("input-1")
        .expect("preset added to known input");

    assert_eq!(slot, 4, "next free slot after the max key (3)");
    let inp = &p.inputs["input-1"];
    assert_eq!(inp.active_preset, 4, "new preset becomes active");
    let name = inp.bank.get(&4).expect("bank has the new slot");
    assert!(
        !["clean", "drive", "lead"].contains(&name.as_str()),
        "unique name, no collision: {name}"
    );
    assert_eq!(p.presets.len(), presets_before + 1, "one new preset");
    let new_preset = &p.presets[name];
    assert!(
        new_preset.blocks.is_empty(),
        "fresh preset: no blocks copied from the active source (got {} blocks)",
        new_preset.blocks.len()
    );
    assert_eq!(
        new_preset.volume, 100.0,
        "fresh preset: default volume, not cloned from the source's 137"
    );
}

#[test]
fn add_preset_to_unknown_input_is_none() {
    let mut p = project_with(vec![("input-1", input(&[(1, "clean")], 1))], &["clean"]);
    assert_eq!(p.add_preset_to_input("nope"), None);
    assert_eq!(p.presets.len(), 1, "nothing added");
}

#[test]
fn add_preset_to_empty_bank_uses_slot_1() {
    let mut p = project_with(vec![("input-1", input(&[], 0))], &[]);
    let slot = p.add_preset_to_input("input-1").expect("added");
    assert_eq!(slot, 1, "first slot in an empty bank");
    assert_eq!(p.inputs["input-1"].active_preset, 1);
    assert_eq!(p.presets.len(), 1, "a blank preset was created");
}

// User repro (#436): there was no way to remove a preset (only the
// whole chain). Remove the active preset from the input's bank; the
// last remaining preset can't be removed; an orphaned pool entry (no
// bank references it) is dropped.
#[test]
fn remove_active_preset_drops_it_and_reactivates_another() {
    let mut p = project_with(
        vec![("input-1", input(&[(1, "a"), (2, "b")], 2))],
        &["a", "b"],
    );
    let now = p.remove_preset_from_input("input-1").expect("removed");
    assert_eq!(now, 1, "falls back to the remaining slot");
    let ri = &p.inputs["input-1"];
    assert!(
        !ri.bank.values().any(|n| n == "b"),
        "b unbanked: {:?}",
        ri.bank
    );
    assert_eq!(ri.active_preset, 1);
    assert!(!p.presets.contains_key("b"), "orphan pool entry dropped");
    assert!(p.presets.contains_key("a"), "still-referenced preset kept");

    assert_eq!(
        p.remove_preset_from_input("input-1"),
        None,
        "can't remove the only remaining preset"
    );
}

#[test]
fn remove_preset_unknown_input_is_none() {
    let mut p = project_with(vec![("input-1", input(&[(1, "a")], 1))], &["a"]);
    assert_eq!(p.remove_preset_from_input("nope"), None);
}

// User repro (#436): deleting a chain didn't update the view — the
// chain came back (rig session re-projects every input). Deleting a
// rig chain must remove the RigInput; its presets are dropped from the
// pool when no other input references them. Unknown input ⇒ false.
#[test]
fn remove_input_drops_it_and_orphaned_presets() {
    let mut p = project_with(
        vec![
            ("in-a", input(&[(1, "a")], 1)),
            ("in-b", input(&[(1, "b")], 1)),
        ],
        &["a", "b"],
    );
    assert!(p.remove_input("in-b"), "removed");
    assert!(!p.inputs.contains_key("in-b"), "input gone");
    assert!(p.inputs.contains_key("in-a"), "other input kept");
    assert!(!p.presets.contains_key("b"), "orphaned preset dropped");
    assert!(p.presets.contains_key("a"), "still-referenced preset kept");
    assert!(!p.remove_input("nope"), "unknown input → false");
}

#[test]
fn step_preset_wraps_both_directions() {
    // bank slots [1,2,3]; active_preset=2 ⇒ position 1 (0-based ordinal).
    let p = project_with(
        vec![("in", input(&[(1, "a"), (2, "b"), (3, "c")], 2))],
        &["a", "b", "c"],
    );
    assert_eq!(p.step_preset("in", 1), Some(2), "next from middle");
    assert_eq!(p.step_preset("in", -1), Some(0), "prev from middle");

    let last = project_with(
        vec![("in", input(&[(1, "a"), (2, "b"), (3, "c")], 3))],
        &["a", "b", "c"],
    );
    assert_eq!(last.step_preset("in", 1), Some(0), "next wraps to first");

    let first = project_with(
        vec![("in", input(&[(1, "a"), (2, "b"), (3, "c")], 1))],
        &["a", "b", "c"],
    );
    assert_eq!(first.step_preset("in", -1), Some(2), "prev wraps to last");
    assert_eq!(
        first.step_preset("missing", 1),
        None,
        "unknown input → None"
    );
}

#[test]
fn step_scene_wraps_within_scene_count() {
    let mut p = project_with(vec![("in", input(&[(1, "a")], 1))], &["a"]);
    let pa = p.presets.get_mut("a").unwrap();
    pa.scenes.insert(2, RigScene::default());
    pa.scenes.insert(3, RigScene::default()); // scene_count = 3

    // active_scene = 1
    assert_eq!(p.step_scene("in", 1), Some(2), "next scene");
    assert_eq!(p.step_scene("in", -1), Some(3), "prev wraps to last scene");

    p.inputs.get_mut("in").unwrap().active_scene = 3;
    assert_eq!(p.step_scene("in", 1), Some(1), "next wraps to first scene");
    assert_eq!(p.step_scene("missing", 1), None, "unknown input → None");
}

/// A `.openrig` YAML without the `instrument` field must deserialize to the default
/// ("electric_guitar") so projects saved before #627 open without error.
#[test]
fn rig_input_missing_instrument_defaults_to_electric_guitar() {
    let yaml = r#"
inputs:
  guitar:
    active-preset: 1
presets: {}
"#;
    let rig: RigProject = serde_yaml::from_str(yaml).unwrap();
    let input = rig.inputs.get("guitar").unwrap();
    assert_eq!(
        input.instrument,
        block_core::DEFAULT_INSTRUMENT,
        "absent instrument field must default to electric_guitar"
    );
}

/// Serialize a `RigProject` with acoustic_guitar, deserialize it; the instrument
/// must survive the full YAML round-trip.
#[test]
fn rig_input_instrument_serde_roundtrip() {
    let mut p = project_with(vec![("guitar", input(&[(1, "a")], 1))], &["a"]);
    p.inputs.get_mut("guitar").unwrap().instrument = block_core::INST_ACOUSTIC_GUITAR.to_string();

    let yaml = serde_yaml::to_string(&p).unwrap();
    let restored: RigProject = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(
        restored.inputs.get("guitar").unwrap().instrument,
        block_core::INST_ACOUSTIC_GUITAR,
        "instrument must survive YAML serialize→deserialize"
    );
}
