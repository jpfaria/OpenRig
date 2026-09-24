//! Issue #978: before VST 3.6.10 a Windows module was a single `Foo.vst3` DLL,
//! not a bundle directory, and many plugins still install that way. Discovery
//! only walked directories, so those plugins never reached the catalog.
#![cfg(target_os = "windows")]

use std::fs;
use std::path::PathBuf;

#[test]
fn a_single_file_module_is_discovered_by_its_name() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("issue_978_single_file");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let module = root.join("Old Delay.vst3");
    fs::write(&module, b"MZ").unwrap();

    let found = vst3_host::scan_vst3_dirs(&[root.clone()]);

    assert_eq!(found.len(), 1, "the single-file module must be listed");
    assert_eq!(found[0].name, "Old Delay");
    assert_eq!(found[0].bundle_path, module);
    let _ = fs::remove_dir_all(&root);
}
