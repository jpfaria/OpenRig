//! #328 — edits made inside a split survive save + reload (spec §1.4): the
//! capture (`sync_synthetic_into_rig`) reaches the path blocks and the split's
//! knobs, a model swap on a path block keeps every scene (#986 inside paths),
//! and adding a block to a path is structural like on the top level.

use std::collections::BTreeMap;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::split_params::{MIX_PAN_A, SPLIT_MODE};
use project::block::{
    find_block_mut, walk_blocks, AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject, RigScene};
use project::rig_sync::sync_synthetic_into_rig;

fn amp(id: &str, model: &str, gain: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(gain));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".into(),
            model: model.into(),
            params,
        }),
    }
}

fn dual_amp(a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            b,
            ..SplitBlock::new(SplitEnd::Mix)
        }),
    }
}

/// Input `g` plays preset `p` = one split, amp_a on A and amp_b on B (gain
/// 0.5 each). Scene 2 bypasses amp_b and pulls its gain to 0.1.
fn rig(active_scene: usize) -> RigProject {
    let mut preset = RigPreset::from_legacy_blocks(
        vec![dual_amp(
            vec![amp("amp_a", "m1", 0.5)],
            vec![amp("amp_b", "m1", 0.5)],
        )],
        100.0,
    );
    preset.scene_params = vec!["amp_b.gain".into()];
    preset.scenes = BTreeMap::from([
        (1, RigScene::default()),
        (
            2,
            RigScene {
                label: None,
                bypass: BTreeMap::from([("amp_b".to_string(), true)]),
                params: BTreeMap::from([("amp_b.gain".to_string(), 0.1)]),
                volume: None,
            },
        ),
    ]);
    RigProject {
        name: None,
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([("p".to_string(), preset)]),
        midi: None,
        chain_order: Vec::new(),
    }
}

/// The projected chain the user edits (the active scene applied, then `edit`),
/// captured back into the rig the way a save does.
fn capture(rig: &mut RigProject, edit: impl FnOnce(&mut Vec<AudioBlock>)) {
    let scene = rig.inputs["g"].active_scene;
    let mut blocks = rig.presets["p"].apply_scene(scene);
    edit(&mut blocks);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("rig:g".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks,
            di_output: None,
            loopers: Vec::new(),
        }],
        midi: None,
    };
    sync_synthetic_into_rig(rig, &project);
}

fn edited<'a>(blocks: &'a mut [AudioBlock], id: &str) -> &'a mut AudioBlock {
    find_block_mut(blocks, id).unwrap_or_else(|| panic!("no block {id}"))
}

fn block<'a>(blocks: &'a [AudioBlock], id: &str) -> &'a AudioBlock {
    walk_blocks(blocks)
        .into_iter()
        .find(|b| b.id.0 == id)
        .unwrap_or_else(|| panic!("no block {id}"))
}

fn gain(block: &AudioBlock) -> Option<f32> {
    match &block.kind {
        AudioBlockKind::Core(c) => c.params.get_f32("gain"),
        _ => None,
    }
}

#[test]
fn a_knob_turned_inside_a_path_is_kept_in_the_active_scene() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| {
        if let AudioBlockKind::Core(c) = &mut edited(blocks, "amp_a").kind {
            c.params.insert("gain", ParameterValue::Float(0.8));
        }
    });
    let preset = &rig.presets["p"];
    assert_eq!(
        preset.scenes[&1].params.get("amp_a.gain"),
        Some(&0.8),
        "the edit is the active scene's override"
    );
    assert!(preset.scene_params.contains(&"amp_a.gain".to_string()));
    assert_eq!(
        gain(block(&preset.apply_scene(2), "amp_a")),
        Some(0.5),
        "scene 2 keeps the preset base"
    );
}

#[test]
fn a_bypass_inside_a_path_is_kept_in_the_active_scene() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| edited(blocks, "amp_a").enabled = false);
    assert_eq!(rig.presets["p"].scenes[&1].bypass.get("amp_a"), Some(&true));
    assert!(
        block(&rig.presets["p"].apply_scene(2), "amp_a").enabled,
        "scene 2 is untouched"
    );
}

#[test]
fn a_mixer_knob_is_a_scene_override_while_the_split_mode_is_preset_wide() {
    let mut rig = rig(1);
    capture(&mut rig, |blocks| {
        if let AudioBlockKind::Split(s) = &mut edited(blocks, "split").kind {
            s.params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
            s.params
                .insert(SPLIT_MODE, ParameterValue::String("dual_mono".into()));
        }
    });
    let preset = &rig.presets["p"];
    assert_eq!(
        preset.scenes[&1].params.get("split.mix_pan_a"),
        Some(&-50.0),
        "a float knob is per scene"
    );
    let AudioBlockKind::Split(base) = &preset.blocks[0].kind else {
        panic!("block 0 is the split")
    };
    assert_eq!(
        base.params.get_string(SPLIT_MODE),
        Some("dual_mono"),
        "a non-float knob is preset-wide (#690)"
    );
}

#[test]
fn a_model_swap_inside_a_path_keeps_every_scene() {
    let mut rig = rig(2);
    capture(&mut rig, |blocks| {
        edited(blocks, "amp_a").kind = amp("amp_a", "m2", 0.3).kind;
    });
    let preset = &rig.presets["p"];
    assert_eq!(
        preset.scenes.len(),
        2,
        "#986: a model swap is not structural, inside a path too"
    );
    let scene_1 = preset.apply_scene(1);
    assert_eq!(
        gain(block(&scene_1, "amp_b")),
        Some(0.5),
        "scene 1 must not inherit scene 2's amp_b values"
    );
    assert!(block(&scene_1, "amp_b").enabled, "nor its bypass");
    let AudioBlockKind::Core(amp_a) = &block(&preset.blocks, "amp_a").kind else {
        panic!("amp_a is a core block")
    };
    assert_eq!(amp_a.model, "m2", "the swapped model is in the preset base");
}

/// Adding a block inside a path is structural, and #986 applies to it exactly
/// like on the top level: the preset keeps every scene and the base of every
/// block it already owned — the active scene is not baked into the base.
#[test]
fn a_block_added_inside_a_path_is_a_structural_edit_that_keeps_every_scene() {
    let mut rig = rig(2);
    capture(&mut rig, |blocks| {
        if let AudioBlockKind::Split(s) = &mut edited(blocks, "split").kind {
            s.a.push(amp("amp_c", "m1", 0.5));
        }
    });
    let preset = &rig.presets["p"];
    let AudioBlockKind::Split(split) = &preset.blocks[0].kind else {
        panic!("block 0 is the split")
    };
    let ids: Vec<&str> = split.a.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(
        ids,
        vec!["amp_a", "amp_c"],
        "the new block is in the preset"
    );
    assert_eq!(preset.scenes.len(), 2, "#986: every scene survives");
    assert_eq!(
        preset.scenes[&2].bypass.get("amp_b"),
        Some(&true),
        "scene 2 keeps its bypass of a path block"
    );
    assert_eq!(
        preset.scenes[&2].params.get("amp_b.gain"),
        Some(&0.1),
        "scene 2 keeps its override of a path block"
    );
    let base_b = block(&preset.blocks, "amp_b");
    assert_eq!(
        gain(base_b),
        Some(0.5),
        "the active scene is not baked into the base"
    );
    assert!(base_b.enabled, "nor its bypass");
    let scene_1 = preset.apply_scene(1);
    assert_eq!(gain(block(&scene_1, "amp_b")), Some(0.5));
    assert!(block(&scene_1, "amp_b").enabled);
}
