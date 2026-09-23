use std::fs;
use std::path::{Path, PathBuf};

use super::catalog_entries;
use crate::discovery::Vst3PluginInfo;

fn info(bundle: &Path, name: &str) -> Vst3PluginInfo {
    Vst3PluginInfo {
        uid: [0u8; 16],
        name: name.to_string(),
        vendor: String::new(),
        category: "Audio Module Class".to_string(),
        bundle_path: bundle.to_path_buf(),
        params: Vec::new(),
        num_audio_inputs: 2,
        num_audio_outputs: 2,
    }
}

/// A bundle carrying a binary for the platform these tests run on, laid out
/// the way `bundle_binary_path` looks it up.
fn bundle_for_this_platform(root: &Path, stem: &str) -> PathBuf {
    let bundle = root.join(format!("{stem}.vst3"));
    let (dir, file) = if cfg!(target_os = "macos") {
        ("MacOS", stem.to_string())
    } else if cfg!(target_os = "windows") {
        ("x86_64-win", format!("{stem}.vst3"))
    } else if cfg!(target_arch = "aarch64") {
        ("aarch64-linux", format!("{stem}.so"))
    } else {
        ("x86_64-linux", format!("{stem}.so"))
    };
    let contents = bundle.join("Contents").join(dir);
    fs::create_dir_all(&contents).unwrap();
    fs::write(contents.join(file), b"").unwrap();
    bundle
}

/// A bundle whose only binary is for a platform other than this one.
fn bundle_for_another_platform(root: &Path, stem: &str) -> PathBuf {
    let bundle = root.join(format!("{stem}.vst3"));
    let dir = if cfg!(target_os = "windows") {
        "MacOS"
    } else {
        "x86_64-win"
    };
    let contents = bundle.join("Contents").join(dir);
    fs::create_dir_all(&contents).unwrap();
    fs::write(contents.join(format!("{stem}.vst3")), b"").unwrap();
    bundle
}

#[test]
fn catalog_leaves_out_bundles_without_a_binary_for_this_platform() {
    // #978: every bundled catalog VST3 ships macOS/Linux binaries only, so on
    // Windows they were listed in the picker and failed when added.
    let root = tempfile::tempdir().unwrap();
    let usable = bundle_for_this_platform(root.path(), "Usable");
    let foreign = bundle_for_another_platform(root.path(), "Foreign");

    let entries = catalog_entries(vec![info(&usable, "Usable"), info(&foreign, "Foreign")]);

    let names: Vec<&str> = entries.iter().map(|e| e.display_name).collect();
    assert_eq!(names, vec!["Usable"]);
}

#[test]
fn catalog_keeps_one_entry_per_model_id() {
    let root = tempfile::tempdir().unwrap();
    let bundle = bundle_for_this_platform(root.path(), "Twice");

    let entries = catalog_entries(vec![info(&bundle, "Twice"), info(&bundle, "Twice")]);

    assert_eq!(entries.len(), 1);
}
