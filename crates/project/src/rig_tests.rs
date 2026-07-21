//! Unit tests for the `project.openrig` model + validation (#449).

use super::*;
use crate::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock};
use crate::param::ParameterSet;
use domain::ids::BlockId;

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
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

pub(super) fn input(bank: &[(usize, &str)], active: usize) -> RigInput {
    RigInput {
        label: None,
        bank: bank.iter().map(|(i, n)| (*i, n.to_string())).collect(),
        active_preset: active,
        active_scene: 1,
        routing: vec![],
        instrument: "electric_guitar".to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: Vec::new(),
    }
}

pub(super) fn project_with(inputs: Vec<(&str, RigInput)>, presets: &[&str]) -> RigProject {
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
fn validate_routing_unknown_output_err() {
    let mut inp = input(&[(1, "clean")], 1);
    inp.routing = vec!["nope".into()];
    let p = project_with(vec![("input-1", inp)], &["clean"]);
    let err = p.validate().unwrap_err();
    assert!(err.contains("nope"), "got: {err}");
}

// ── #454 T1: scenes model + validation + backward-compat ──────────────────

pub(super) fn core_block(id: &str) -> AudioBlock {
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
