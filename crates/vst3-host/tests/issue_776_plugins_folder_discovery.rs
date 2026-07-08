//! Issue #776: a catalog VST3 bundle shipped under the OpenRig plugins folder
//! must be discovered exactly like a system-installed VST3. This proves the
//! discovery scan recurses a plugins-root-shaped tree
//! (`<root>/vst3/<id>/bundles/<Name>.vst3`) and returns the same
//! catalog-shaped `Vst3PluginInfo` the system scan yields — so it flows through
//! the identical catalog / block-kind / native-editor path.

use std::fs;
use std::path::PathBuf;

#[test]
fn scan_vst3_dirs_discovers_a_bundle_in_the_plugins_folder_layout() {
    // Build <tmp>/vst3/chow_centaur/bundles/ChowCentaur.vst3 with a
    // moduleinfo.json — the same nested layout OpenRig-plugins ships.
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("issue_776_plugins_root");
    let _ = fs::remove_dir_all(&base);
    let vst3_root = base.join("vst3");
    let bundle = vst3_root
        .join("chow_centaur")
        .join("bundles")
        .join("ChowCentaur.vst3");
    let resources = bundle.join("Contents").join("Resources");
    fs::create_dir_all(&resources).expect("create fixture bundle");
    fs::write(
        resources.join("moduleinfo.json"),
        r#"{
  "Factory Info": { "Vendor": "chowdsp" },
  "Classes": [
    {
      "CID": "0123456789ABCDEF0123456789ABCDEF",
      "Category": "Audio Module Class",
      "Name": "ChowCentaur"
    }
  ]
}"#,
    )
    .expect("write moduleinfo.json");

    let found = vst3_host::scan_vst3_dirs(&[vst3_root]);

    assert_eq!(found.len(), 1, "the nested .vst3 bundle must be discovered");
    let info = &found[0];
    assert_eq!(info.name, "ChowCentaur");
    assert_eq!(info.vendor, "chowdsp");
    assert_ne!(info.uid, [0u8; 16], "moduleinfo CID must resolve the uid");
    assert!(info.bundle_path.ends_with("ChowCentaur.vst3"));
}
