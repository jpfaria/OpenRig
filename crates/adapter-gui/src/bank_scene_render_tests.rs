//! Tests for the pure navigator render mapper (#453). No Slint/AppWindow.

use super::*;
use crate::bank_scene_session::BankSceneState;
use project::rig::{RigInput, RigProject};
use std::collections::BTreeMap;

fn rig() -> RigProject {
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: Some("Eu".into()),
            sources: vec![],
            bank: BTreeMap::from([(1, "clean".to_string()), (4, "lead".to_string())]),
            active_preset: 4,
            active_scene: 3,
            routing: vec![],
        },
    );
    inputs.insert(
        "input-2".to_string(),
        RigInput {
            label: None,
            sources: vec![],
            bank: BTreeMap::from([(2, "drive".to_string())]),
            active_preset: 2,
            active_scene: 1,
            routing: vec![],
        },
    );
    RigProject {
        name: Some("Studio".into()),
        inputs,
        outputs: BTreeMap::new(),
        presets: BTreeMap::new(),
        midi: None,
    }
}

#[test]
fn render_mirrors_state_in_order_with_selected_flag() {
    let state = BankSceneState::from_project(&rig());
    let rows = render(&state);
    assert_eq!(rows.len(), 2);

    assert_eq!(rows[0].input, "input-1");
    assert_eq!(rows[0].label, "Eu");
    assert_eq!(rows[0].active_preset, 4);
    assert_eq!(rows[0].active_scene, 3);
    assert_eq!(rows[0].bank_slots, vec![1, 4], "sorted, gap preserved");
    assert!(rows[0].selected, "first input auto-selected");

    assert_eq!(rows[1].input, "input-2");
    assert_eq!(
        rows[1].label, "",
        "missing label → empty (screen falls back)"
    );
    assert!(!rows[1].selected);
}

#[test]
fn render_tracks_selection_change() {
    let mut state = BankSceneState::from_project(&rig());
    state.apply(crate::bank_scene_session::BankSceneEvent::SelectInput(
        "input-2".into(),
    ));
    let rows = render(&state);
    assert!(!rows[0].selected);
    assert!(rows[1].selected, "selection moved to input-2");
}

#[test]
fn render_empty_project_is_empty() {
    let state = BankSceneState::from_project(&RigProject {
        name: None,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        presets: BTreeMap::new(),
        midi: None,
    });
    assert!(render(&state).is_empty());
}
