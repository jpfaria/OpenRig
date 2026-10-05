//! #986 — changing a block's model rewrote the whole preset from the live
//! chain. The owner changed the amp model of preset "IGREJA - VST3" (chain
//! `input-7`) while scene 3 was active; afterwards the preset had a single
//! empty scene 1, `scene-params` was empty, `active-scene` was back to 1 and
//! the values the live chain carried were baked into the base.
//!
//! Intended behavior pinned here: a model change replaces THAT block only. It
//! drops the scene overrides (and `scene-params` entries) of that block that
//! the new model no longer has, and nothing else. Every scene, every other
//! override, every bypass, the base values of the other blocks and the active
//! scene survive — including after the edits are captured into the rig, which
//! is what a save does.
//!
//! The fixture mirrors the shape of the owner's preset (same scenes, same
//! bypass/override layout) with native models so it loads anywhere.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::rig::{RigPreset, RigProject};
use serde_yaml::Value;

use application::command::{BlockCommand, Command, ProjectCommand, RigNavKind, SelectionCommand};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const INPUT: &str = "input-7";
const CHAIN: &str = "rig:input-7";
const PRESET: &str = "anal-dig";
const AMP: &str = "rig:input-7:block:amp";
const NEW_MODEL: &str = "chime";
/// A scene override on the amp that the new model does not have.
const STALE_KEY: &str = "rig:input-7:block:amp.character";

fn fixture_rig() -> RigProject {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/issue_986_scened_preset.yaml");
    infra_yaml::load_project_file(&path).expect("load the #986 fixture rig")
}

fn preset(rig: &RigProject) -> RigPreset {
    rig.presets.get(PRESET).expect("fixture preset").clone()
}

fn yaml<T: serde::Serialize>(v: &T) -> Value {
    serde_yaml::to_value(v).expect("serialize")
}

/// A dispatcher over the rig, the chain projected at `active_scene`.
fn session(active_scene: usize) -> (LocalDispatcher, Rc<RefCell<RigProject>>) {
    let mut rig = fixture_rig();
    rig.inputs.get_mut(INPUT).unwrap().active_scene = active_scene;
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig,
        &BTreeSet::new(),
    )));
    let rig = Rc::new(RefCell::new(rig));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (dispatcher, rig)
}

fn change_amp_model(d: &LocalDispatcher) {
    d.dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
        chain: ChainId(CHAIN.into()),
        block: BlockId(AMP.into()),
        model_id: NEW_MODEL.into(),
    }))
    .expect("ReplaceBlockModel");
}

fn capture(d: &LocalDispatcher) {
    d.dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("CaptureRigEdits");
}

/// The preset the fixture holds, with only what the model change is allowed
/// to remove taken out: the amp's override the new model does not have.
fn expected_scenes_and_scene_params(before: &RigPreset) -> (Value, Value) {
    let mut scenes = yaml(&before.scenes);
    if let Value::Mapping(map) = &mut scenes {
        for (_, scene) in map.iter_mut() {
            if let Some(Value::Mapping(params)) = scene.get_mut("params") {
                params.remove(STALE_KEY);
            }
        }
    }
    let scene_params: Vec<String> = before
        .scene_params
        .iter()
        .filter(|k| k.as_str() != STALE_KEY)
        .cloned()
        .collect();
    (scenes, yaml(&scene_params))
}

fn assert_only_the_amp_changed(before: &RigPreset, after: &RigPreset) {
    let (scenes, scene_params) = expected_scenes_and_scene_params(before);
    assert_eq!(
        yaml(&after.scenes),
        scenes,
        "every scene, bypass and override must survive a model change; only \
         the amp override the new model lacks may go"
    );
    assert_eq!(
        yaml(&after.scene_params),
        scene_params,
        "scene-params must keep every entry but the one the new model lacks"
    );
    assert_eq!(after.blocks.len(), before.blocks.len(), "same block list");
    for (b, a) in before.blocks.iter().zip(after.blocks.iter()) {
        assert_eq!(a.id, b.id, "block order must not change");
        if a.id.0 == AMP {
            assert_eq!(
                a.enabled, b.enabled,
                "the amp's base enabled flag is the preset's, not the live scene's"
            );
            match &a.kind {
                project::block::AudioBlockKind::Core(core) => {
                    assert_eq!(core.model, NEW_MODEL, "the amp got the new model")
                }
                other => panic!("amp must stay a core block, got {}", other.label()),
            }
        } else {
            assert_eq!(
                yaml(a),
                yaml(b),
                "block {} is not the one whose model changed; its base (enabled \
                 flag and values) must be exactly what the preset held",
                a.id.0
            );
        }
    }
}

#[test]
fn model_change_on_scene_3_keeps_every_scene_after_capture() {
    let before = preset(&fixture_rig());
    let (d, rig) = session(3);

    change_amp_model(&d);
    capture(&d);

    let rig = rig.borrow();
    assert_eq!(
        rig.inputs[INPUT].active_scene, 3,
        "the model change must not send the chain back to scene 1"
    );
    assert_only_the_amp_changed(&before, &preset(&rig));
}

#[test]
fn model_change_keeps_every_scene_even_before_capture() {
    let before = preset(&fixture_rig());
    let (d, rig) = session(3);

    change_amp_model(&d);

    let rig = rig.borrow();
    assert_eq!(rig.inputs[INPUT].active_scene, 3);
    let after = preset(&rig);
    let (scenes, scene_params) = expected_scenes_and_scene_params(&before);
    assert_eq!(yaml(&after.scenes), scenes);
    assert_eq!(yaml(&after.scene_params), scene_params);
}

#[test]
fn model_change_after_switching_from_scene_1_to_3_keeps_every_scene() {
    // The owner's session: the chain was on scene 1, then scene 3, then the
    // amp model changed. Scene 1's overrides must not leak into the base.
    let before = preset(&fixture_rig());
    let (d, rig) = session(1);
    d.dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
        chain: ChainId(CHAIN.into()),
        kind: RigNavKind::Scene(3),
    }))
    .expect("switch to scene 3");

    change_amp_model(&d);
    capture(&d);

    let rig = rig.borrow();
    assert_eq!(rig.inputs[INPUT].active_scene, 3);
    assert_only_the_amp_changed(&before, &preset(&rig));
}

#[test]
fn fixture_precondition_new_model_keeps_input_and_output_but_not_character() {
    let keys: Vec<String> = application::block_factory::default_params_for_model("amp", NEW_MODEL)
        .expect("new model params")
        .values
        .keys()
        .cloned()
        .collect();
    assert!(keys.iter().any(|k| k == "input"));
    assert!(keys.iter().any(|k| k == "output"));
    assert!(!keys.iter().any(|k| k == "character"));
}

/// The block editor swaps a model through `OverwriteBlock` (the editor builds
/// the new block itself and carries the live `enabled`), not
/// `ReplaceBlockModel`. The capture a save runs must treat it the same way.
#[test]
fn model_change_through_the_block_editor_keeps_every_scene_after_capture() {
    let before = preset(&fixture_rig());
    let (d, rig) = session(3);
    let params = application::block_factory::default_params_for_model("amp", NEW_MODEL)
        .expect("new model params");
    d.dispatch(Command::Block(BlockCommand::OverwriteBlock {
        chain: ChainId(CHAIN.into()),
        block: BlockId(AMP.into()),
        replacement: project::block::AudioBlock {
            id: BlockId(AMP.into()),
            enabled: false,
            kind: project::block::AudioBlockKind::Core(project::block::CoreBlock {
                effect_type: "amp".into(),
                model: NEW_MODEL.into(),
                params,
            }),
        },
    }))
    .expect("OverwriteBlock");
    capture(&d);

    let rig = rig.borrow();
    assert_eq!(rig.inputs[INPUT].active_scene, 3);
    assert_only_the_amp_changed(&before, &preset(&rig));
}

/// Swapping on the scene that overrides the amp keeps that scene's surviving
/// overrides, both in the preset and on the live block.
#[test]
fn model_change_on_the_scene_that_overrides_the_block_keeps_its_overrides() {
    let before = preset(&fixture_rig());
    let (d, rig) = session(2);

    change_amp_model(&d);
    capture(&d);

    assert_only_the_amp_changed(&before, &preset(&rig.borrow()));
}
