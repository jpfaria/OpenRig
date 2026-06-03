//! Red-first tests for `RigCommand`: validate the BUSINESS (each command
//! mutates the rig correctly) and the CLICK→COMMAND mapping (the Slint
//! sentinel int turns into the right command), so neither needs manual QA.

use super::{rig_command_from_scene, rig_command_from_select, RigCommand};
use crate::block::InputEntry;
use crate::chain::ChainInputMode;
use crate::rig::{RigInput, RigPreset, RigProject};
use domain::ids::DeviceId;
use std::collections::BTreeMap;

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    for n in ["clean", "drive", "lead"] {
        presets.insert(n.to_string(), RigPreset::from_legacy_blocks(vec![], 100.0));
    }
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId("d".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank: BTreeMap::from([
                (1, "clean".to_string()),
                (2, "drive".to_string()),
                (3, "lead".to_string()),
            ]),
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
        },
    );
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

// ── Business: each command mutates the rig correctly ──────────────────────

#[test]
fn switch_preset_command_activates_the_selected_combobox_position() {
    let mut r = rig(); // active_preset 1 (clean)
    RigCommand::SwitchPreset {
        input: "in".into(),
        position: 1, // 2nd row = bank key 2 = "drive"
    }
    .apply(&mut r)
    .expect("valid");
    assert_eq!(r.inputs["in"].active_preset, 2, "position 1 → bank key 2");
}

// #535: scenes are per-preset; switching preset must NOT carry the
// previous preset's active_scene over (a stale index leaks into the
// next preset on the very next write_back_processing_blocks call and
// materializes a phantom scene there).
#[test]
fn switch_preset_command_resets_active_scene_to_one() {
    let mut r = rig();
    r.add_scene_to_input("in").expect("scene 2 in clean");
    assert_eq!(r.inputs["in"].active_scene, 2, "we leave clean on scene 2");

    RigCommand::SwitchPreset {
        input: "in".into(),
        position: 1, // → "drive"
    }
    .apply(&mut r)
    .expect("valid");

    assert_eq!(
        r.inputs["in"].active_scene, 1,
        "switching presets must reset active_scene so the new preset starts on its own scene 1"
    );
}

#[test]
fn add_preset_command_appends_and_activates() {
    let mut r = rig();
    RigCommand::AddPreset { input: "in".into() }
        .apply(&mut r)
        .expect("valid");
    assert_eq!(r.inputs["in"].active_preset, 4, "new slot active");
    assert_eq!(r.presets.len(), 4, "one preset added");
}

#[test]
fn switch_scene_command_sets_active_scene() {
    let mut r = rig();
    RigCommand::SwitchScene {
        input: "in".into(),
        scene: 1,
    }
    .apply(&mut r)
    .expect("valid");
    r.add_scene_to_input("in"); // make scene 2 exist
    RigCommand::SwitchScene {
        input: "in".into(),
        scene: 2,
    }
    .apply(&mut r)
    .expect("valid");
    assert_eq!(r.inputs["in"].active_scene, 2);
    assert_eq!(
        RigCommand::SwitchScene {
            input: "in".into(),
            scene: 9
        }
        .apply(&mut r),
        None,
        "scene out of 1..=8 is rejected"
    );
}

#[test]
fn add_scene_then_remove_scene_commands_grow_and_pop() {
    let mut r = rig();
    RigCommand::AddScene { input: "in".into() }
        .apply(&mut r)
        .expect("valid");
    assert_eq!(r.inputs["in"].active_scene, 2);
    assert_eq!(r.presets["clean"].scene_count(), 2);

    RigCommand::RemoveScene { input: "in".into() }
        .apply(&mut r)
        .expect("valid");
    assert_eq!(r.presets["clean"].scene_count(), 1);
    assert_eq!(
        RigCommand::RemoveScene { input: "in".into() }.apply(&mut r),
        None,
        "can't remove the only remaining scene"
    );
}

#[test]
fn unknown_input_command_is_none() {
    let mut r = rig();
    assert_eq!(
        RigCommand::SwitchPreset {
            input: "nope".into(),
            position: 0
        }
        .apply(&mut r),
        None
    );
}

// ── Click → command mapping (validates "o clique chama o command certo") ──

#[test]
fn select_int_maps_to_preset_command() {
    assert_eq!(
        rig_command_from_select("in", 2),
        RigCommand::SwitchPreset {
            input: "in".into(),
            position: 2
        }
    );
    assert_eq!(
        rig_command_from_select("in", -1),
        RigCommand::AddPreset { input: "in".into() }
    );
    assert_eq!(
        rig_command_from_select("in", -2),
        RigCommand::RemovePreset { input: "in".into() }
    );
}

#[test]
fn remove_preset_command_drops_active_and_reactivates() {
    let mut r = rig(); // bank {1 clean,2 drive,3 lead}, active 1
    RigCommand::RemovePreset { input: "in".into() }
        .apply(&mut r)
        .expect("removed");
    assert!(
        !r.inputs["in"].bank.values().any(|n| n == "clean"),
        "active preset removed"
    );
    assert_ne!(r.inputs["in"].active_preset, 1, "reactivated another slot");
}

#[test]
fn scene_int_maps_to_scene_command() {
    assert_eq!(
        rig_command_from_scene("in", 3),
        RigCommand::SwitchScene {
            input: "in".into(),
            scene: 3
        }
    );
    assert_eq!(
        rig_command_from_scene("in", -1),
        RigCommand::AddScene { input: "in".into() }
    );
    assert_eq!(
        rig_command_from_scene("in", -2),
        RigCommand::RemoveScene { input: "in".into() }
    );
}
