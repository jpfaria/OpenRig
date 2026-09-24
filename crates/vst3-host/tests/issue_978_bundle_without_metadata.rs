//! Issue #978: a native Windows VST3 bundle usually carries neither
//! `Contents/Resources/moduleinfo.json` (pre-SDK 3.7) nor a macOS
//! `Contents/Info.plist`, so discovery used to bail on it and those plugins
//! never reached the catalog. On Windows the bundle's own name now names the
//! plugin, and its class id resolves lazily from the factory.
//!
//! Elsewhere nothing changes: a macOS or Linux bundle with neither file is
//! still skipped (a Windows fix stays on Windows; on the owner's Mac this is
//! Melodyne and ReValver, whose Info.plist has no CFBundleName).

use std::fs;
use std::path::PathBuf;

fn bundle_without_metadata(label: &str) -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(label);
    let _ = fs::remove_dir_all(&root);
    let bundle = root.join("Plain Reverb.vst3");
    fs::create_dir_all(bundle.join("Contents").join("x86_64-win")).unwrap();
    (root, bundle)
}

#[cfg(target_os = "windows")]
#[test]
fn a_bundle_without_metadata_is_discovered_by_its_name_on_windows() {
    let (root, bundle) = bundle_without_metadata("issue_978_no_metadata_win");

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

#[cfg(not(target_os = "windows"))]
#[test]
fn a_bundle_without_metadata_is_still_skipped_off_windows() {
    let (root, bundle) = bundle_without_metadata("issue_978_no_metadata_other");

    assert!(
        vst3_host::scan_vst3_bundle_light(&bundle).is_err(),
        "macOS/Linux keep skipping a bundle with no moduleinfo.json and no CFBundleName"
    );
    let _ = fs::remove_dir_all(&root);
}
