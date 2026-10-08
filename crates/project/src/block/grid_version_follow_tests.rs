use super::*;
use domain::value_objects::ParameterValue as P;
use std::path::{Path, PathBuf};

const PRESET_GRID: &str = "
parameters:
  - name: preset
    values: [Cap, Cone]
captures:
  - values: { preset: Cap }
    file: captures/000.wav
  - values: { preset: Cone }
    file: captures/001.wav
";

const NAMED_GRID: &str = "
parameters:
  - name: distance
    values: [0, 2]
captures:
  - values: { distance: 0 }
    file: captures/000.wav
  - values: { distance: 2 }
    file: captures/001.wav
";

const RENAMED_GRID: &str = "
parameters:
  - name: dist
    values: [0, 2]
captures:
  - values: { dist: 0 }
    file: captures/000.wav
  - values: { dist: 2 }
    file: captures/001.wav
";

fn manifest_yaml(id: &str, grid: &str) -> String {
    format!(
        "manifest_version: 1\nid: {id}\ndisplay_name: Follow Cab\ntype: cab\nbackend: ir\n{grid}"
    )
}

/// A package on disk with `current` as its manifest and `versions` saved
/// beside it, loaded into the (process-wide) catalog under `id`.
fn install(id: &str, current: &str, versions: &[&str]) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("openrig-grid-follow-{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join(id);
    std::fs::create_dir_all(dir.join("captures")).unwrap();
    for file in ["000.wav", "001.wav"] {
        std::fs::write(dir.join("captures").join(file), b"RIFF").unwrap();
    }
    std::fs::write(dir.join("manifest.yaml"), manifest_yaml(id, current)).unwrap();
    for (i, grid) in versions.iter().enumerate() {
        let manifest: plugin_loader::PluginManifest =
            serde_yaml::from_str(&manifest_yaml(id, grid)).unwrap();
        plugin_loader::version_store::write_version(&dir, i as u32 + 1, &manifest).unwrap();
    }
    plugin_loader::registry::load_one(id, &[root.clone()]).unwrap();
    root
}

fn params(values: &[(&str, P)]) -> ParameterSet {
    let mut set = ParameterSet::default();
    for (k, v) in values {
        set.insert(*k, v.clone());
    }
    set
}

fn cleanup(root: &Path, id: &str) {
    let _ = plugin_loader::registry::unload(id);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_block_saved_before_an_edit_moves_to_the_new_values_on_load() {
    let id = "follow_test_one_edit";
    let root = install(id, NAMED_GRID, &[PRESET_GRID, NAMED_GRID]);
    let saved = params(&[
        ("preset", P::String("Cone".into())),
        ("output_db", P::Float(-2.0)),
    ]);

    let loaded = follow_saved_grid_versions(id, saved);

    assert_eq!(loaded.get("distance"), Some(&P::Float(2.0)));
    assert_eq!(loaded.get("preset"), None);
    assert_eq!(loaded.get("output_db"), Some(&P::Float(-2.0)));
    cleanup(&root, id);
}

#[test]
fn a_block_saved_two_edits_ago_still_finds_its_capture() {
    let id = "follow_test_two_edits";
    let root = install(id, RENAMED_GRID, &[PRESET_GRID, NAMED_GRID, RENAMED_GRID]);

    let loaded = follow_saved_grid_versions(id, params(&[("preset", P::String("Cap".into()))]));

    assert_eq!(loaded.get("dist"), Some(&P::Float(0.0)));
    assert_eq!(loaded.get("preset"), None);
    cleanup(&root, id);
}

#[test]
fn a_block_that_already_matches_the_current_grid_is_left_alone() {
    let id = "follow_test_current";
    let root = install(id, NAMED_GRID, &[PRESET_GRID, NAMED_GRID]);
    let saved = params(&[
        ("distance", P::Float(2.0)),
        ("preset", P::String("Cap".into())),
    ]);

    assert_eq!(follow_saved_grid_versions(id, saved.clone()), saved);
    cleanup(&root, id);
}

#[test]
fn a_plugin_without_saved_versions_is_left_alone() {
    let id = "follow_test_no_versions";
    let root = install(id, NAMED_GRID, &[]);
    let saved = params(&[("preset", P::String("Cone".into()))]);

    assert_eq!(follow_saved_grid_versions(id, saved.clone()), saved);
    cleanup(&root, id);
}

#[test]
fn a_model_that_is_not_in_the_catalog_is_left_alone() {
    let saved = params(&[("preset", P::String("Cone".into()))]);
    assert_eq!(
        follow_saved_grid_versions("follow_test_missing", saved.clone()),
        saved
    );
}
