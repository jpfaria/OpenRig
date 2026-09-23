use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::package_runs_on;
use crate::discover::LoadedPackage;
use crate::manifest::{Lv2Slot, PluginManifest};

/// Per-test package directory, removed on drop.
struct PackageDir(PathBuf);

impl PackageDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "openrig-platform-support-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create package dir");
        Self(path)
    }
}

impl Drop for PackageDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn package(root: &PackageDir, yaml: &str) -> LoadedPackage {
    let manifest: PluginManifest = serde_yaml::from_str(yaml).expect("manifest parses");
    LoadedPackage {
        root: root.0.clone(),
        manifest,
    }
}

// Mirrors the 51 bundled LV2 packages that ship macOS and Linux binaries only.
const LV2_WITHOUT_WINDOWS: &str = r#"
manifest_version: 1
id: lv2_no_windows
display_name: No Windows
type: reverb
backend: lv2
plugin_uri: urn:example:no-windows
binaries:
  macos-universal: platform/macos-universal/plugin.dylib
  linux-x86_64: platform/linux-x86_64/plugin.so
"#;

const VST3_BUNDLE: &str = r#"
manifest_version: 1
id: vst3_example
display_name: Example
type: vst3
backend: vst3
bundle: bundles/Example.vst3
parameters: []
"#;

const NAM_PACKAGE: &str = r#"
manifest_version: 1
id: nam_example
display_name: Example
type: preamp
backend: nam
parameters:
  - name: gain
    values: [10]
captures:
  - values: { gain: 10 }
    file: captures/g10.nam
"#;

#[test]
fn lv2_without_windows_binary_does_not_run_on_windows() {
    let dir = PackageDir::new("lv2-win");
    let pkg = package(&dir, LV2_WITHOUT_WINDOWS);
    assert!(!package_runs_on(&pkg, Some(Lv2Slot::WindowsX86_64)));
}

#[test]
fn lv2_runs_where_it_ships_a_binary() {
    let dir = PackageDir::new("lv2-mac");
    let pkg = package(&dir, LV2_WITHOUT_WINDOWS);
    assert!(package_runs_on(&pkg, Some(Lv2Slot::MacosUniversal)));
    assert!(package_runs_on(&pkg, Some(Lv2Slot::LinuxX86_64)));
}

#[test]
fn lv2_does_not_run_on_an_unsupported_platform() {
    let dir = PackageDir::new("lv2-none");
    let pkg = package(&dir, LV2_WITHOUT_WINDOWS);
    assert!(!package_runs_on(&pkg, None));
}

#[test]
fn vst3_bundle_without_windows_binary_does_not_run_on_windows() {
    // The 26 bundled VST3 packages: Contents/MacOS and *-linux, no x86_64-win.
    let dir = PackageDir::new("vst3-win");
    let contents = dir.0.join("bundles/Example.vst3/Contents");
    fs::create_dir_all(contents.join("MacOS")).unwrap();
    fs::create_dir_all(contents.join("x86_64-linux")).unwrap();
    let pkg = package(&dir, VST3_BUNDLE);
    assert!(!package_runs_on(&pkg, Some(Lv2Slot::WindowsX86_64)));
    assert!(package_runs_on(&pkg, Some(Lv2Slot::MacosUniversal)));
    assert!(package_runs_on(&pkg, Some(Lv2Slot::LinuxX86_64)));
}

#[test]
fn vst3_bundle_with_windows_binary_runs_on_windows() {
    let dir = PackageDir::new("vst3-win-ok");
    fs::create_dir_all(dir.0.join("bundles/Example.vst3/Contents/x86_64-win")).unwrap();
    let pkg = package(&dir, VST3_BUNDLE);
    assert!(package_runs_on(&pkg, Some(Lv2Slot::WindowsX86_64)));
    assert!(!package_runs_on(&pkg, Some(Lv2Slot::MacosUniversal)));
}

#[test]
fn capture_packages_run_everywhere() {
    // NAM/IR captures are data files the engine loads on every platform.
    let dir = PackageDir::new("nam");
    let pkg = package(&dir, NAM_PACKAGE);
    for slot in [
        Some(Lv2Slot::WindowsX86_64),
        Some(Lv2Slot::MacosUniversal),
        Some(Lv2Slot::LinuxAarch64),
        None,
    ] {
        assert!(package_runs_on(&pkg, slot), "{slot:?}");
    }
}
