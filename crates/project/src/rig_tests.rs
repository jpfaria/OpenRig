//! Unit tests for the `project.openrig` model + validation (#449).

use super::*;
use crate::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock};
use crate::chain::ChainInputMode;
use crate::param::ParameterSet;
use domain::ids::{BlockId, DeviceId};

fn source(device: &str, channels: Vec<usize>) -> InputEntry {
    InputEntry {
        device_id: DeviceId(device.into()),
        mode: ChainInputMode::Mono,
        channels,
    }
}

fn processing_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("blk:od".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "tape".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn io_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("blk:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![source("dev", vec![0])],
        }),
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
        chain_order: Vec::new(),
        presets: presets
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    RigPreset {
                        id: String::new(),
                        name: None,
                        blocks: vec![processing_block()],
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

#[test]
fn validate_ok_minimal() {
    let p = project_with(vec![("input-1", input(&[(1, "clean")], 1))], &["clean"]);
    assert!(p.validate().is_ok());
}

#[test]
fn validate_bank_references_missing_preset_err() {
    let p = project_with(vec![("input-1", input(&[(1, "ghost")], 1))], &[]);
    let err = p.validate().unwrap_err();
    assert!(err.contains("ghost"), "got: {err}");
}

#[test]
fn validate_active_preset_not_in_bank_err() {
    let p = project_with(vec![("input-1", input(&[(1, "clean")], 9))], &["clean"]);
    let err = p.validate().unwrap_err();
    assert!(err.contains("active-preset"), "got: {err}");
}

#[test]
fn validate_scene_out_of_range_err() {
    let mut inp = input(&[(1, "clean")], 1);
    inp.active_scene = 9;
    let p = project_with(vec![("input-1", inp)], &["clean"]);
    let err = p.validate().unwrap_err();
    assert!(err.contains("scene"), "got: {err}");
}

#[test]
fn validate_preset_with_io_block_err() {
    let mut p = project_with(vec![("input-1", input(&[(1, "clean")], 1))], &["clean"]);
    p.presets.get_mut("clean").unwrap().blocks.push(io_block());
    let err = p.validate().unwrap_err();
    assert!(err.contains("I/O"), "got: {err}");
}

#[test]
fn validate_source_channel_conflict_err() {
    let mut inp = input(&[(1, "clean")], 1);
    inp.sources = vec![source("scarlett", vec![0]), source("scarlett", vec![0])];
    let p = project_with(vec![("input-1", inp)], &["clean"]);
    let err = p.validate().unwrap_err();
    assert!(
        err.contains("Channel 0") && err.contains("scarlett"),
        "got: {err}"
    );
}

#[test]
fn validate_routing_unknown_output_err() {
    let mut inp = input(&[(1, "clean")], 1);
    inp.routing = vec!["nope".into()];
    let p = project_with(vec![("input-1", inp)], &["clean"]);
    let err = p.validate().unwrap_err();
    assert!(err.contains("nope"), "got: {err}");
}

// ── #454 T1: scenes model + validation + backward-compat ──────────────────

fn core_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "tape".into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn scene_or_default_empty_preset_returns_default_for_slot_1() {
    let p = RigPreset {
        id: String::new(),
        name: None,
        blocks: vec![],
        scene_params: vec![],
        scenes: BTreeMap::new(),
        volume: 100.0,
    };
    assert_eq!(p.scene_or_default(1), RigScene::default());
}

#[test]
fn scene_or_default_returns_present_scene() {
    let scene = RigScene {
        label: Some("solo".into()),
        bypass: BTreeMap::from([("od".to_string(), true)]),
        params: BTreeMap::new(),
        volume: None,
    };
    let p = RigPreset {
        id: String::new(),
        name: None,
        blocks: vec![core_block("od")],
        scene_params: vec![],
        scenes: BTreeMap::from([(2, scene.clone())]),
        volume: 100.0,
    };
    assert_eq!(p.scene_or_default(2), scene);
}

#[test]
fn validate_scene_index_out_of_range_err() {
    let mut p = project_with(vec![("input-1", input(&[(1, "clean")], 1))], &["clean"]);
    p.presets.get_mut("clean").unwrap().scenes = BTreeMap::from([(9, RigScene::default())]);
    let err = p.validate().unwrap_err();
    assert!(err.contains("scene"), "got: {err}");
}

#[test]
fn validate_scene_param_not_marked_err() {
    let mut p = project_with(vec![("input-1", input(&[(1, "clean")], 1))], &["clean"]);
    let preset = p.presets.get_mut("clean").unwrap();
    preset.blocks = vec![core_block("od")];
    preset.scene_params = vec![]; // nothing marked
    preset.scenes = BTreeMap::from([(
        1,
        RigScene {
            label: None,
            bypass: BTreeMap::new(),
            params: BTreeMap::from([("od.gain".to_string(), 0.7)]),
            volume: None,
        },
    )]);
    let err = p.validate().unwrap_err();
    assert!(
        err.contains("od.gain") && err.contains("scene-param"),
        "got: {err}"
    );
}

// ── #454 T2: pure apply_scene (bypass + marked-param override) ─────────────

fn preset_with(
    blocks: Vec<AudioBlock>,
    marked: &[&str],
    scenes: Vec<(usize, RigScene)>,
) -> RigPreset {
    RigPreset {
        id: String::new(),
        name: None,
        blocks,
        scene_params: marked.iter().map(|s| s.to_string()).collect(),
        scenes: scenes.into_iter().collect(),
        volume: 100.0,
    }
}

#[test]
fn apply_scene_no_scenes_returns_blocks_unchanged() {
    let p = preset_with(vec![core_block("od")], &[], vec![]);
    let out = p.apply_scene(1);
    assert_eq!(out, p.blocks, "pre-scenes preset = identity Default scene");
}

#[test]
fn apply_scene_bypass_disables_named_block() {
    let p = preset_with(
        vec![core_block("od"), core_block("dl")],
        &[],
        vec![(
            1,
            RigScene {
                label: None,
                bypass: BTreeMap::from([("od".to_string(), true)]),
                params: BTreeMap::new(),
                volume: None,
            },
        )],
    );
    let out = p.apply_scene(1);
    assert!(!out[0].enabled, "od bypassed in scene 1");
    assert!(out[1].enabled, "dl untouched");
}

#[test]
fn apply_scene_bypass_false_keeps_block_enabled() {
    let p = preset_with(
        vec![core_block("od")],
        &[],
        vec![(
            1,
            RigScene {
                label: None,
                bypass: BTreeMap::from([("od".to_string(), false)]),
                params: BTreeMap::new(),
                volume: None,
            },
        )],
    );
    assert!(p.apply_scene(1)[0].enabled);
}

#[test]
fn apply_scene_overrides_only_marked_param() {
    let p = preset_with(
        vec![core_block("od")],
        &["od.gain"],
        vec![(
            2,
            RigScene {
                label: None,
                bypass: BTreeMap::new(),
                params: BTreeMap::from([("od.gain".to_string(), 0.7)]),
                volume: None,
            },
        )],
    );
    let out = p.apply_scene(2);
    let params = match &out[0].kind {
        AudioBlockKind::Core(c) => &c.params,
        _ => unreachable!(),
    };
    assert_eq!(params.get_f32("gain"), Some(0.7));
}

#[test]
fn apply_scene_ignores_param_not_in_scene_params() {
    // Defensive: even if a scene carries an unmarked key, apply_scene only
    // applies keys listed in scene_params (validate also rejects this).
    let p = preset_with(
        vec![core_block("od")],
        &[], // nothing marked
        vec![(
            1,
            RigScene {
                label: None,
                bypass: BTreeMap::new(),
                params: BTreeMap::from([("od.gain".to_string(), 0.9)]),
                volume: None,
            },
        )],
    );
    let out = p.apply_scene(1);
    let params = match &out[0].kind {
        AudioBlockKind::Core(c) => &c.params,
        _ => unreachable!(),
    };
    assert_eq!(params.get_f32("gain"), None, "unmarked param not applied");
}

#[test]
fn validate_scene_param_marked_ok() {
    let mut p = project_with(vec![("input-1", input(&[(1, "clean")], 1))], &["clean"]);
    let preset = p.presets.get_mut("clean").unwrap();
    preset.blocks = vec![core_block("od")];
    preset.scene_params = vec!["od.gain".into()];
    preset.scenes = BTreeMap::from([(
        1,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("od".to_string(), true)]),
            params: BTreeMap::from([("od.gain".to_string(), 0.7)]),
            volume: None,
        },
    )]);
    assert!(p.validate().is_ok(), "{:?}", p.validate());
}

#[test]
fn validate_allows_inputs_sharing_a_tap_at_rest() {
    // Cross-input tap exclusivity is a RUNTIME concern, not a static one:
    // a project may hold many inputs sharing a (device, channel) as a
    // library of alternative configs. The engine refuses to *activate*
    // two conflicting inputs together — validate() does not reject them.
    let mut a = input(&[(1, "clean")], 1);
    a.sources = vec![source("scarlett", vec![0])];
    let mut b = input(&[(1, "clean")], 1);
    b.sources = vec![source("scarlett", vec![0])]; // same tap, on purpose
    let p = project_with(vec![("input-1", a), ("input-2", b)], &["clean"]);

    assert!(p.validate().is_ok(), "{:?}", p.validate());
}

#[test]
fn format_version_constants_are_one() {
    assert_eq!(crate::rig::PROJECT_FORMAT_VERSION, 1);
    assert_eq!(crate::rig::PRESET_FORMAT_VERSION, 1);
}

#[test]
fn from_legacy_blocks_preserves_blocks_and_volume_no_scenes() {
    let blocks = vec![core_block("od"), core_block("amp")];
    let preset = RigPreset::from_legacy_blocks(blocks.clone(), 137.0);

    assert_eq!(preset.blocks, blocks, "blocks bit-identical and in order");
    assert_eq!(preset.volume, 137.0, "volume preserved exact");
    assert!(preset.scene_params.is_empty());
    assert!(preset.scenes.is_empty());
    // Behaves as one Default scene that changes nothing (back-compat).
    assert_eq!(preset.apply_scene(1), blocks);
}

// #627 — replacing a block's MODEL (ReplaceBlockModel) keeps the block id but
// changes effect_type/model. The structural check compared only ids, so the
// swap was treated as a non-structural per-scene diff and the new model was
// never written into the preset base — the pedal reverted on reload.
#[test]
fn replace_preset_blocks_detects_same_id_model_swap_as_structural() {
    fn core_model_block(id: &str, effect_type: &str, model: &str) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: effect_type.into(),
                model: model.into(),
                params: ParameterSet::default(),
            }),
        }
    }

    let mut p = project_with(vec![("input-1", input(&[(1, "p")], 1))], &["p"]);
    p.presets.get_mut("p").unwrap().blocks = vec![core_model_block("b1", "gain", "ts9")];

    // Same id "b1", new model — exactly what ReplaceBlockModel produces.
    let swapped = core_model_block("b1", "gain", "klon");
    let structural = p.replace_preset_blocks_if_structural("input-1", &[swapped]);

    assert!(
        structural,
        "a same-id model swap must count as structural so the preset base is replaced"
    );
    let base_model = match &p.presets.get("p").unwrap().blocks[0].kind {
        AudioBlockKind::Core(c) => c.model.clone(),
        _ => "??".into(),
    };
    assert_eq!(
        base_model, "klon",
        "model swap must be written into the preset base block, not dropped"
    );
}

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
