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
        presets: presets
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    RigPreset {
                        blocks: vec![processing_block()],
                        scene_params: vec![],
                        scenes: BTreeMap::new(),
                    },
                )
            })
            .collect(),
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
        blocks: vec![],
        scene_params: vec![],
        scenes: BTreeMap::new(),
    };
    assert_eq!(p.scene_or_default(1), RigScene::default());
}

#[test]
fn scene_or_default_returns_present_scene() {
    let scene = RigScene {
        label: Some("solo".into()),
        bypass: BTreeMap::from([("od".to_string(), true)]),
        params: BTreeMap::new(),
    };
    let p = RigPreset {
        blocks: vec![core_block("od")],
        scene_params: vec![],
        scenes: BTreeMap::from([(2, scene.clone())]),
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
        },
    )]);
    let err = p.validate().unwrap_err();
    assert!(
        err.contains("od.gain") && err.contains("scene-param"),
        "got: {err}"
    );
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
        },
    )]);
    assert!(p.validate().is_ok(), "{:?}", p.validate());
}
