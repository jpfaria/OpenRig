//! #1093: the plugin tree the installers bundle lives in this repo at
//! `plugins/source/`. Every package there must load cleanly (manifest parses,
//! declared files exist) and must be an open-source LV2 or VST3 plugin — NAM
//! and IR captures live in the private OpenRig-plugins repo and never ship.
//! The release `bundle-plugins` job runs this test as its validation gate.

use std::path::PathBuf;

use plugin_loader::{discover, Backend};

fn bundled_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/source")
}

#[test]
fn every_bundled_package_loads() {
    let root = bundled_root();
    assert!(root.is_dir(), "{} is missing", root.display());
    let results = discover(&root).expect("read plugins/source");
    let failures: Vec<String> = results
        .iter()
        .filter_map(|r| r.as_ref().err().map(|e| e.to_string()))
        .collect();
    assert!(
        failures.is_empty(),
        "invalid packages:\n{}",
        failures.join("\n")
    );
    assert!(
        results.len() > 100,
        "expected the full LV2/VST3 catalog, found {} package(s)",
        results.len()
    );
}

#[test]
fn bundled_tree_ships_only_lv2_and_vst3() {
    let results = discover(&bundled_root()).expect("read plugins/source");
    let foreign: Vec<String> = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .filter(|p| {
            !matches!(
                p.manifest.backend,
                Backend::Lv2 { .. } | Backend::Vst3 { .. }
            )
        })
        .map(|p| p.root.display().to_string())
        .collect();
    assert!(
        foreign.is_empty(),
        "non LV2/VST3 packages in the bundle:\n{}",
        foreign.join("\n")
    );
}
