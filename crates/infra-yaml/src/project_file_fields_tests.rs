//! Field-level save + reload tests for the project `.yaml` (#1014). Each
//! field the chain model carried in the removed chain document is pinned
//! here on the rig document that replaced it.

use super::*;
use project::rig::{RigInput, RigPreset, RigProject};
use std::collections::BTreeMap;
use tempfile::tempdir;

fn input(preset: &str, instrument: &str, io_binding_ids: Vec<String>) -> RigInput {
    RigInput {
        label: None,
        bank: BTreeMap::from([(1, preset.to_string())]),
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
        instrument: instrument.to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids,
        loopers: Vec::new(),
        disabled_endpoints: Default::default(),
        mix: Default::default(),
        di_output: None,
    }
}

fn preset(id: &str, volume: f32) -> RigPreset {
    RigPreset {
        id: id.to_string(),
        name: None,
        blocks: Vec::new(),
        scene_params: Vec::new(),
        scenes: BTreeMap::new(),
        volume,
    }
}

fn rig(inputs: Vec<(&str, RigInput)>, presets: Vec<RigPreset>) -> RigProject {
    RigProject {
        name: None,
        inputs: inputs
            .into_iter()
            .map(|(id, input)| (id.to_string(), input))
            .collect(),
        outputs: BTreeMap::new(),
        presets: presets.into_iter().map(|p| (p.id.clone(), p)).collect(),
        midi: None,
        chain_order: Vec::new(),
    }
}

fn save_and_reload(project: &RigProject) -> RigProject {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("project.yaml");
    save_project_file(&path, project).expect("save");
    load_project_file(&path).expect("load")
}

#[test]
fn empty_project_round_trips() {
    let empty = rig(Vec::new(), Vec::new());
    assert_eq!(save_and_reload(&empty), empty);
}

#[test]
fn every_input_keeps_its_own_instrument() {
    let project = rig(
        vec![
            ("guitar", input("g", "electric_guitar", Vec::new())),
            ("bass", input("b", "bass", Vec::new())),
        ],
        vec![preset("g", 100.0), preset("b", 100.0)],
    );
    let loaded = save_and_reload(&project);
    assert_eq!(loaded.inputs.len(), 2);
    assert_eq!(loaded.inputs["guitar"].instrument, "electric_guitar");
    assert_eq!(loaded.inputs["bass"].instrument, "bass");
}

#[test]
fn an_input_without_instrument_defaults_to_the_shared_constant() {
    let loaded = parse_project(
        "project:\n  inputs:\n    in-1:\n      bank: { 1: p }\n      active-preset: 1\n  outputs: {}\n  presets:\n    p:\n      blocks: []\n",
    )
    .expect("parse");
    assert_eq!(
        loaded.inputs["in-1"].instrument,
        block_core::DEFAULT_INSTRUMENT
    );
}

#[test]
fn preset_volume_150_round_trips() {
    // #440: the volume on disk must never be reset to unity on reload.
    let project = rig(
        vec![("in-1", input("p", "electric_guitar", Vec::new()))],
        vec![preset("p", 150.0)],
    );
    assert_eq!(save_and_reload(&project).presets["p"].volume, 150.0);
}

#[test]
fn preset_without_volume_defaults_to_unity() {
    let loaded = parse_project(
        "project:\n  inputs:\n    in-1:\n      bank: { 1: p }\n      active-preset: 1\n  outputs: {}\n  presets:\n    p:\n      blocks: []\n",
    )
    .expect("parse");
    assert_eq!(loaded.presets["p"].volume, 100.0);
}

#[test]
fn io_binding_ids_round_trip() {
    // #716: the E/S bindings an input selected survive save + reopen.
    let bindings = vec!["main".to_string(), "fx".to_string()];
    let project = rig(
        vec![("in-1", input("p", "electric_guitar", bindings.clone()))],
        vec![preset("p", 100.0)],
    );
    assert_eq!(
        save_and_reload(&project).inputs["in-1"].io_binding_ids,
        bindings
    );
}

#[test]
fn input_without_io_binding_ids_defaults_to_empty() {
    let loaded = parse_project(
        "project:\n  inputs:\n    in-1:\n      bank: { 1: p }\n      active-preset: 1\n  outputs: {}\n  presets:\n    p:\n      blocks: []\n",
    )
    .expect("parse");
    assert!(loaded.inputs["in-1"].io_binding_ids.is_empty());
}

#[test]
fn load_fails_on_invalid_yaml() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("bad.yaml");
    std::fs::write(&path, "{{{{not valid yaml!!!!").expect("write");
    assert!(load_project_file(&path).is_err());
}

#[test]
fn load_fails_on_a_missing_file() {
    let dir = tempdir().expect("temp dir");
    assert!(load_project_file(&dir.path().join("absent.yaml")).is_err());
}
