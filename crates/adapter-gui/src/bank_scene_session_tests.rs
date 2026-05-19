//! Pure navigation-core tests (#453). No Slint, no AppWindow.

use super::*;
use project::rig::{RigInput, RigProject};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn rig() -> RigProject {
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: Some("Eu".into()),
            sources: vec![],
            bank: BTreeMap::from([
                (1, "clean".to_string()),
                (3, "drive".to_string()),
                (5, "lead".to_string()),
            ]),
            active_preset: 3,
            active_scene: 2,
            routing: vec![],
        },
    );
    inputs.insert(
        "input-2".to_string(),
        RigInput {
            label: None,
            sources: vec![],
            bank: BTreeMap::from([(1, "clean".to_string())]),
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
        },
    );
    RigProject {
        name: Some("Studio".into()),
        inputs,
        outputs: BTreeMap::new(),
        presets: BTreeMap::new(),
    }
}

#[test]
fn from_project_derives_per_input_nav_with_sorted_gapped_bank() {
    let s = BankSceneState::from_project(&rig());
    assert!(s.project_open);
    assert_eq!(s.inputs.len(), 2);
    let i1 = s.input("input-1").unwrap();
    assert_eq!(i1.bank_slots, vec![1, 3, 5], "sorted, gaps preserved");
    assert_eq!(i1.active_preset, 3);
    assert_eq!(i1.active_scene, 2);
    // first input auto-selected for convenience
    assert_eq!(s.selected_input.as_deref(), Some("input-1"));
}

#[test]
fn select_input_must_exist() {
    let mut s = BankSceneState::from_project(&rig());
    assert!(s
        .apply(BankSceneEvent::SelectInput("ghost".into()))
        .is_empty());
    assert_eq!(s.selected_input.as_deref(), Some("input-1"));
    let fx = s.apply(BankSceneEvent::SelectInput("input-2".into()));
    assert!(fx.is_empty(), "selection alone emits no engine effect");
    assert_eq!(s.selected_input.as_deref(), Some("input-2"));
}

#[test]
fn bank_next_prev_walks_gapped_slots_and_emits_switch_preset() {
    let mut s = BankSceneState::from_project(&rig());
    // input-1 active slot = 3 (middle of [1,3,5])
    let fx = s.apply(BankSceneEvent::BankNext);
    assert_eq!(s.input("input-1").unwrap().active_preset, 5);
    assert_eq!(
        fx,
        vec![BankSceneEffect::SwitchPreset {
            input: "input-1".into(),
            slot: 5
        }]
    );
    // clamp at end (no wrap)
    let fx = s.apply(BankSceneEvent::BankNext);
    assert_eq!(s.input("input-1").unwrap().active_preset, 5);
    assert!(fx.is_empty(), "clamped: no change, no effect");
    s.apply(BankSceneEvent::BankPrev);
    s.apply(BankSceneEvent::BankPrev);
    assert_eq!(s.input("input-1").unwrap().active_preset, 1);
    let fx = s.apply(BankSceneEvent::BankPrev);
    assert!(fx.is_empty(), "clamped at start");
}

#[test]
fn select_slot_only_accepts_existing_bank_slot() {
    let mut s = BankSceneState::from_project(&rig());
    let fx = s.apply(BankSceneEvent::SelectSlot(4));
    assert!(fx.is_empty(), "slot 4 not in bank [1,3,5]");
    assert_eq!(s.input("input-1").unwrap().active_preset, 3);
    let fx = s.apply(BankSceneEvent::SelectSlot(1));
    assert_eq!(
        fx,
        vec![BankSceneEffect::SwitchPreset {
            input: "input-1".into(),
            slot: 1
        }]
    );
    assert_eq!(s.input("input-1").unwrap().active_preset, 1);
}

#[test]
fn scene_navigation_clamps_1_to_8_and_emits_switch_scene() {
    let mut s = BankSceneState::from_project(&rig());
    // input-1 active_scene = 2
    let fx = s.apply(BankSceneEvent::SelectScene(5));
    assert_eq!(
        fx,
        vec![BankSceneEffect::SwitchScene {
            input: "input-1".into(),
            scene: 5
        }]
    );
    assert_eq!(s.input("input-1").unwrap().active_scene, 5);
    assert!(s.apply(BankSceneEvent::SelectScene(9)).is_empty());
    assert!(s.apply(BankSceneEvent::SelectScene(0)).is_empty());
    assert_eq!(
        s.input("input-1").unwrap().active_scene,
        5,
        "invalid ignored"
    );
    s.apply(BankSceneEvent::SelectScene(8));
    let fx = s.apply(BankSceneEvent::SceneNext);
    assert!(fx.is_empty(), "clamped at 8");
    s.apply(BankSceneEvent::SelectScene(1));
    assert!(
        s.apply(BankSceneEvent::ScenePrev).is_empty(),
        "clamped at 1"
    );
}

#[test]
fn bank_scene_acts_on_selected_input_only_shared_timbre() {
    // "same input = shared timbre": switching the selected input's slot does
    // NOT touch the other input — each input has its own bank/scene.
    let mut s = BankSceneState::from_project(&rig());
    s.apply(BankSceneEvent::SelectInput("input-2".into()));
    let before_i1 = s.input("input-1").unwrap().clone();
    let fx = s.apply(BankSceneEvent::SelectScene(4));
    assert_eq!(
        fx,
        vec![BankSceneEffect::SwitchScene {
            input: "input-2".into(),
            scene: 4
        }]
    );
    assert_eq!(&before_i1, s.input("input-1").unwrap(), "input-1 untouched");
}

#[test]
fn open_and_create_project_emit_intent_effects_only() {
    let mut s = BankSceneState::from_project(&rig());
    let p = PathBuf::from("/x/project.openrig");
    assert_eq!(
        s.apply(BankSceneEvent::OpenProject(p.clone())),
        vec![BankSceneEffect::OpenProject(p.clone())]
    );
    assert_eq!(
        s.apply(BankSceneEvent::CreateProject(p.clone())),
        vec![BankSceneEffect::CreateProject(p)]
    );
}

#[test]
fn no_project_state_has_no_inputs() {
    let empty = RigProject {
        name: None,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        presets: BTreeMap::new(),
    };
    let s = BankSceneState::from_project(&empty);
    assert!(s.inputs.is_empty());
    assert!(s.selected_input.is_none());
}
