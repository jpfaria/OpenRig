//! #780: a scanned catalog VST3 bundle finds its owning OpenRig package
//! manifest (walking up from the raw `.vst3` folder) and reads the declared
//! `vst3_id → group` map. Fixture is built under `CARGO_TARGET_TMPDIR` so the
//! test touches no machine paths and cleans up after itself.

use std::fs;
use std::path::PathBuf;

use plugin_loader::vst3_group_map_for_bundle;

fn fixture_dir(name: &str) -> PathBuf {
    // Cargo sets CARGO_TARGET_TMPDIR for integration tests. Scope each dir by
    // test name so the parallel tests never clobber each other's fixture on
    // cleanup.
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    base.join(format!("vst3_group_map_{}_{}", std::process::id(), name))
}

#[test]
fn reads_group_map_from_owning_package_manifest() {
    // Package layout: <root>/vst3/chow/manifest.yaml + the scanned bundle at
    // <root>/vst3/chow/bundles/ChowCentaur.vst3/ (two levels below the manifest).
    let root = fixture_dir("owning");
    let pkg = root.join("vst3").join("chow");
    let bundle = pkg.join("bundles").join("ChowCentaur.vst3");
    fs::create_dir_all(&bundle).expect("create bundle dir");
    fs::write(
        pkg.join("manifest.yaml"),
        r#"
manifest_version: 1
id: chow_centaur
display_name: Chow Centaur
type: vst3
backend: vst3
bundle: bundles/ChowCentaur.vst3
parameters:
  - name: gain
    vst3_id: 0
    min: 0.0
    max: 100.0
    default: 50.0
    group: Tone
  - name: mode
    vst3_id: 5
    min: 0.0
    max: 100.0
    default: 0.0
    group: Voicing
"#,
    )
    .expect("write manifest");

    let map = vst3_group_map_for_bundle(&bundle);
    assert_eq!(map.get(&0).map(String::as_str), Some("Tone"));
    assert_eq!(map.get(&5).map(String::as_str), Some("Voicing"));
    assert_eq!(map.len(), 2);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn returns_empty_when_bundle_has_no_owning_manifest() {
    // A bundle with no package manifest anywhere above it → empty map, so the
    // caller falls back to dynamic grouping.
    let root = fixture_dir("orphan");
    let bundle = root.join("Standalone.vst3");
    fs::create_dir_all(&bundle).expect("create bundle dir");

    let map = vst3_group_map_for_bundle(&bundle);
    assert!(map.is_empty());

    let _ = fs::remove_dir_all(root);
}
