//! Red-first tests for `Query::ListProjectPresets` — the in-memory
//! `RigProject.presets` pool (#554 follow-up). Presets in the pool
//! can exist without being bound to any input bank yet; the disk
//! library is a separate concept tracked elsewhere.

use std::collections::BTreeMap;

use project::rig::{RigPreset, RigProject};

use crate::query::list_project_presets;

fn rig_with_pool(presets: Vec<&str>) -> RigProject {
    let mut pool = BTreeMap::new();
    for name in presets {
        pool.insert(
            name.to_string(),
            RigPreset {
                id: name.to_string(),
                name: Some(name.to_string()),
                blocks: Vec::new(),
                scene_params: Vec::new(),
                scenes: BTreeMap::new(),
                volume: 100.0,
            },
        );
    }
    RigProject {
        name: None,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        presets: pool,
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn list_project_presets_returns_pool_sorted() {
    let rig = rig_with_pool(vec!["Clean strum", "Coldplay - Clocks", "Lead"]);
    let json = list_project_presets(&rig);
    let pos_clean = json.find("Clean strum").expect("Clean strum");
    let pos_clocks = json.find("Coldplay - Clocks").expect("Coldplay - Clocks");
    let pos_lead = json.find("Lead").expect("Lead");
    assert!(pos_clean < pos_clocks);
    assert!(pos_clocks < pos_lead);
}

#[test]
fn list_project_presets_includes_pool_entries_even_without_bank_reference() {
    // The whole point of this query: a preset can sit in
    // `RigProject.presets` without being referenced by any input's
    // bank yet. Tone-builder Step 0 must see it anyway, otherwise it
    // overwrites it on save.
    let rig = rig_with_pool(vec!["Coldplay - Clocks"]);
    let json = list_project_presets(&rig);
    assert!(
        json.contains("Coldplay - Clocks"),
        "pool entry must be listed even with empty inputs, got: {json}"
    );
}

#[test]
fn list_project_presets_empty_pool_returns_empty_array() {
    let rig = rig_with_pool(vec![]);
    let json = list_project_presets(&rig);
    assert!(
        json.contains("\"presets\":[]"),
        "empty pool yields empty array, got: {json}"
    );
}

#[test]
fn list_project_presets_is_deterministic() {
    let rig = rig_with_pool(vec!["B", "A", "C"]);
    let a = list_project_presets(&rig);
    let b = list_project_presets(&rig);
    assert_eq!(a, b, "two reads of the same rig produce identical bytes");
}

#[test]
fn list_project_presets_escapes_special_chars_in_names() {
    let rig = rig_with_pool(vec!["With \"quotes\"", "back\\slash", "tab\there"]);
    let json = list_project_presets(&rig);
    assert!(
        json.contains("\\\"quotes\\\""),
        "quotes escaped, got: {json}"
    );
    assert!(
        json.contains("back\\\\slash"),
        "backslash escaped, got: {json}"
    );
    assert!(json.contains("tab\\there"), "tab escaped, got: {json}");
}
