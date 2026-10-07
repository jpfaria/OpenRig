//! #1093 — a dev run from the checkout scans `<cwd>/plugins`, where the
//! versioned tree keeps the VST3 packages under `source/vst3/`, not under
//! `vst3/`. The catalog must still find every VST3 package the plugin loader
//! found, wherever the package sits under the root; otherwise every VST3
//! block fails with "not found in catalog".

use super::vst3_scan_dirs;
use std::path::{Path, PathBuf};

fn write_vst3_package(package: &Path, bundle_name: &str) -> PathBuf {
    let bundle = package.join("bundles").join(format!("{bundle_name}.vst3"));
    std::fs::create_dir_all(bundle.join("Contents")).unwrap();
    std::fs::write(
        bundle.join("Contents/Info.plist"),
        format!(
            "<plist><dict><key>CFBundleName</key><string>{bundle_name}</string></dict></plist>"
        ),
    )
    .unwrap();
    std::fs::write(
        package.join("manifest.yaml"),
        format!(
            "manifest_version: 1\nid: vst3_probe\ndisplay_name: Probe\nbrand: test\n\
             type: vst3\nbackend: vst3\nbundle: bundles/{bundle_name}.vst3\nparameters: []\n"
        ),
    )
    .unwrap();
    bundle
}

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("openrig-1093-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn discovered_bundles(root: &Path) -> Vec<PathBuf> {
    let packages: Vec<_> = plugin_loader::discover(root)
        .unwrap()
        .into_iter()
        .map(|loaded| loaded.expect("fixture package loads"))
        .collect();
    let dirs = vst3_scan_dirs(&[root.to_path_buf()], &packages);
    vst3_host::scan_vst3_dirs(&dirs)
        .into_iter()
        .map(|info| info.bundle_path.canonicalize().unwrap())
        .collect()
}

#[test]
fn a_vst3_package_under_source_is_scanned() {
    let root = temp_root("source").join("plugins");
    let bundle = write_vst3_package(&root.join("source/vst3/probe"), "Probe");
    assert_eq!(
        discovered_bundles(&root),
        vec![bundle.canonicalize().unwrap()]
    );
}

#[test]
fn a_vst3_package_directly_under_vst3_is_still_scanned() {
    let root = temp_root("flat").join("plugins");
    let bundle = write_vst3_package(&root.join("vst3/probe"), "Probe");
    let found = discovered_bundles(&root);
    assert!(found.contains(&bundle.canonicalize().unwrap()), "{found:?}");
}
