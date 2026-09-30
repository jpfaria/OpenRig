//! #986 (reopened) — the owner, on chain `input-7` (ANAL+DIG), preset
//! "DUMBLE BLUE - AMBIENCE" (an `Insert` block plus three scenes):
//!
//! 1. switched scene and every scene of the preset was gone;
//! 2. the insert showed up in every preset of the chain;
//! 3. removing the insert from one preset removed it from every preset of
//!    the chain.
//!
//! Intended behavior pinned here: a preset owns its blocks. An edit on the
//! active preset — adding or removing a block, the insert included — lands
//! in that preset only; the other presets of the bank keep exactly what
//! they held. Switching scene or preset never rewrites any preset's scenes.
//!
//! The fixture mirrors the owner's bank layout with native models.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::rig::{RigPreset, RigProject};
use serde_yaml::Value;

use application::command::{BlockCommand, Command, ProjectCommand, RigNavKind, SelectionCommand};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const INPUT: &str = "input-7";
const CHAIN: &str = "rig:input-7";
const DUMBLE: &str = "anal-dig";
const MARSHALL: &str = "marshall";
const TEST: &str = "test";
const INSERT: &str = "rig:input-7:insert:5";

fn fixture_rig() -> RigProject {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/issue_986_insert_bank.yaml");
    infra_yaml::load_project_file(&path).expect("load the #986 insert-bank fixture")
}

fn preset(rig: &RigProject, key: &str) -> RigPreset {
    rig.presets.get(key).expect("fixture preset").clone()
}

fn yaml<T: serde::Serialize>(v: &T) -> Value {
    serde_yaml::to_value(v).expect("serialize")
}

fn session_over(rig: RigProject) -> (LocalDispatcher, Rc<RefCell<RigProject>>) {
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig,
        &BTreeSet::new(),
    )));
    let rig = Rc::new(RefCell::new(rig));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (dispatcher, rig)
}

fn nav(d: &LocalDispatcher, kind: RigNavKind) {
    d.dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
        chain: ChainId(CHAIN.into()),
        kind,
    }))
    .expect("ApplyRigNav");
}

fn capture(d: &LocalDispatcher) {
    d.dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("CaptureRigEdits");
}

fn insert_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: "syn2-main".into(),
        }),
    }
}

fn insert_ids(p: &RigPreset) -> Vec<String> {
    p.blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Insert(_)))
        .map(|b| b.id.0.clone())
        .collect()
}

#[test]
fn switching_scenes_keeps_every_scene_of_the_preset() {
    let before = preset(&fixture_rig(), DUMBLE);
    let (d, rig) = session_over(fixture_rig());

    nav(&d, RigNavKind::Scene(2));
    nav(&d, RigNavKind::Scene(3));
    nav(&d, RigNavKind::Scene(1));
    capture(&d);

    let after = preset(&rig.borrow(), DUMBLE);
    assert_eq!(
        yaml(&after.scenes),
        yaml(&before.scenes),
        "switching scene must not rewrite the preset's scenes"
    );
    assert_eq!(yaml(&after.scene_params), yaml(&before.scene_params));
    assert_eq!(yaml(&after.blocks), yaml(&before.blocks), "base untouched");
}

#[test]
fn switching_scene_after_a_capture_on_another_scene_keeps_every_scene() {
    let before = preset(&fixture_rig(), DUMBLE);
    let (d, rig) = session_over(fixture_rig());

    nav(&d, RigNavKind::Scene(2));
    capture(&d);
    nav(&d, RigNavKind::Scene(3));
    capture(&d);

    let after = preset(&rig.borrow(), DUMBLE);
    assert_eq!(yaml(&after.scenes), yaml(&before.scenes));
    assert_eq!(yaml(&after.scene_params), yaml(&before.scene_params));
    assert_eq!(yaml(&after.blocks), yaml(&before.blocks));
}

/// The owner's "I changed scene and everything was gone": arriving at DUMBLE
/// BLUE from a preset of the same bank, then only switching scenes — no block
/// edit at all — must leave the preset exactly as it was.
#[test]
fn switching_scene_after_arriving_from_another_preset_keeps_every_scene() {
    let before = preset(&fixture_rig(), DUMBLE);
    let mut start = fixture_rig();
    start.inputs.get_mut(INPUT).unwrap().active_preset = 2;
    let (d, rig) = session_over(start);

    nav(&d, RigNavKind::Preset(0));
    nav(&d, RigNavKind::Scene(2));
    nav(&d, RigNavKind::Scene(3));
    capture(&d);

    let after = preset(&rig.borrow(), DUMBLE);
    assert_eq!(
        yaml(&after.scenes),
        yaml(&before.scenes),
        "only switching scenes must not touch the preset's scenes"
    );
    assert_eq!(yaml(&after.scene_params), yaml(&before.scene_params));
    assert_eq!(
        yaml(&after.blocks),
        yaml(&before.blocks),
        "blocks untouched"
    );
}

#[test]
fn the_insert_of_one_preset_never_appears_in_the_other_presets() {
    let fixture = fixture_rig();
    let (d, rig) = session_over(fixture_rig());

    nav(&d, RigNavKind::Scene(2));
    nav(&d, RigNavKind::Preset(1));
    capture(&d);
    nav(&d, RigNavKind::Preset(2));
    capture(&d);
    nav(&d, RigNavKind::Preset(0));
    capture(&d);

    let rig = rig.borrow();
    for key in [MARSHALL, TEST] {
        assert_eq!(
            yaml(&preset(&rig, key).blocks),
            yaml(&preset(&fixture, key).blocks),
            "preset {key} never had the insert; switching presets must not give it one"
        );
    }
    assert_eq!(insert_ids(&preset(&rig, DUMBLE)), vec![INSERT.to_string()]);
}

#[test]
fn adding_an_insert_to_one_preset_leaves_the_other_presets_alone() {
    let fixture = fixture_rig();
    let (d, rig) = session_over(fixture_rig());

    nav(&d, RigNavKind::Preset(2));
    d.dispatch(Command::Block(BlockCommand::InsertPrebuiltBlock {
        chain: ChainId(CHAIN.into()),
        block: insert_block("rig:input-7:insert:9"),
        position: 0,
    }))
    .expect("add insert");
    capture(&d);
    nav(&d, RigNavKind::Preset(1));
    capture(&d);

    let rig = rig.borrow();
    assert_eq!(
        insert_ids(&preset(&rig, TEST)),
        vec!["rig:input-7:insert:9".to_string()],
        "the insert lands in the preset it was added to"
    );
    assert_eq!(
        yaml(&preset(&rig, MARSHALL).blocks),
        yaml(&preset(&fixture, MARSHALL).blocks),
        "MARSHALL was not edited"
    );
    assert_eq!(
        yaml(&preset(&rig, DUMBLE).blocks),
        yaml(&preset(&fixture, DUMBLE).blocks),
        "DUMBLE BLUE was not edited"
    );
}

#[test]
fn removing_the_insert_from_one_preset_leaves_the_other_presets_alone() {
    let mut start = fixture_rig();
    start
        .presets
        .get_mut(MARSHALL)
        .unwrap()
        .blocks
        .insert(0, insert_block("rig:input-7:insert:6"));
    let fixture = start.clone();
    let (d, rig) = session_over(start);

    d.dispatch(Command::Block(BlockCommand::RemoveBlock {
        chain: ChainId(CHAIN.into()),
        block: BlockId(INSERT.into()),
    }))
    .expect("remove the insert");
    capture(&d);
    nav(&d, RigNavKind::Preset(1));
    capture(&d);
    nav(&d, RigNavKind::Preset(0));
    capture(&d);

    let rig = rig.borrow();
    assert!(
        insert_ids(&preset(&rig, DUMBLE)).is_empty(),
        "the insert was removed from DUMBLE BLUE"
    );
    assert_eq!(
        yaml(&preset(&rig, MARSHALL).blocks),
        yaml(&preset(&fixture, MARSHALL).blocks),
        "MARSHALL keeps its own insert"
    );
    assert_eq!(
        yaml(&preset(&rig, TEST).blocks),
        yaml(&preset(&fixture, TEST).blocks),
        "TEST was not edited"
    );
}

#[test]
fn removing_the_insert_keeps_every_scene_of_the_preset() {
    let before = preset(&fixture_rig(), DUMBLE);
    let (d, rig) = session_over(fixture_rig());

    d.dispatch(Command::Block(BlockCommand::RemoveBlock {
        chain: ChainId(CHAIN.into()),
        block: BlockId(INSERT.into()),
    }))
    .expect("remove the insert");
    capture(&d);
    nav(&d, RigNavKind::Scene(2));
    capture(&d);

    let after = preset(&rig.borrow(), DUMBLE);
    let mut expected = yaml(&before.scenes);
    if let Value::Mapping(scenes) = &mut expected {
        for (_, scene) in scenes.iter_mut() {
            if let Some(Value::Mapping(bypass)) = scene.get_mut("bypass") {
                bypass.remove(INSERT);
            }
        }
    }
    assert_eq!(
        yaml(&after.scenes),
        expected,
        "only the removed insert's bypass entries may leave the scenes"
    );
    assert_eq!(yaml(&after.scene_params), yaml(&before.scene_params));
    assert_eq!(rig.borrow().inputs[INPUT].active_scene, 2);
}
