//! Issue #978: a native Windows VST3 bundle usually carries neither
//! `Contents/Resources/moduleinfo.json` (pre-SDK 3.7) nor a macOS
//! `Contents/Info.plist`. Discovery used to bail on such a bundle, so those
//! plugins never reached the catalog. The bundle itself names the plugin; its
//! class id is resolved lazily from the factory when it is first used.

use std::fs;
use std::path::PathBuf;

#[test]
fn a_bundle_without_metadata_is_discovered_by_its_name() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("issue_978_no_metadata");
    let _ = fs::remove_dir_all(&root);
    let bundle = root.join("Plain Reverb.vst3");
    fs::create_dir_all(bundle.join("Contents").join("x86_64-win")).unwrap();

    let found = vst3_host::scan_vst3_bundle_light(&bundle).expect("bundle is discovered");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Plain Reverb");
    assert_eq!(
        found[0].uid, [0u8; 16],
        "unknown until the factory is asked, like the Info.plist fallback"
    );
    assert_eq!(found[0].bundle_path, bundle);
    let _ = fs::remove_dir_all(&root);
}
