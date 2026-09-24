//! Issue #978: a package with no binary for the host platform loads into the
//! registry (#477: each OS package strips the other platforms' binaries), but
//! it cannot be built here, so it must not be reported as available. On
//! Windows that was 51 bundled LV2 effects and every bundled VST3, listed in
//! the picker and failing the moment they were added.

use std::fs;
use std::path::Path;

use plugin_loader::{current_platform_slot, Lv2Slot};

const ALL_SLOTS: [(Lv2Slot, &str, &str); 5] = [
    (Lv2Slot::MacosUniversal, "macos-universal", "MacOS"),
    (Lv2Slot::WindowsX86_64, "windows-x86_64", "x86_64-win"),
    (Lv2Slot::WindowsAarch64, "windows-aarch64", "arm64-win"),
    (Lv2Slot::LinuxX86_64, "linux-x86_64", "x86_64-linux"),
    (Lv2Slot::LinuxAarch64, "linux-aarch64", "aarch64-linux"),
];

fn host() -> (&'static str, &'static str) {
    let host = current_platform_slot().expect("tests run on a supported platform");
    let (_, slot, vst3_dir) = ALL_SLOTS.iter().find(|(s, _, _)| *s == host).unwrap();
    (slot, vst3_dir)
}

fn foreign() -> (&'static str, &'static str) {
    let host = current_platform_slot().unwrap();
    let (_, slot, vst3_dir) = ALL_SLOTS.iter().find(|(s, _, _)| *s != host).unwrap();
    (slot, vst3_dir)
}

fn write_lv2(root: &Path, id: &str, slot: &str) {
    let dir = root.join("lv2").join(id);
    let binary = format!("platform/{slot}/plugin.bin");
    fs::create_dir_all(dir.join(format!("platform/{slot}"))).unwrap();
    fs::write(dir.join(&binary), b"").unwrap();
    fs::write(
        dir.join("manifest.yaml"),
        format!(
            "manifest_version: 1\nid: {id}\ndisplay_name: {id}\ntype: reverb\n\
             backend: lv2\nplugin_uri: urn:test:{id}\nbinaries:\n  {slot}: {binary}\n"
        ),
    )
    .unwrap();
}

fn write_vst3(root: &Path, id: &str, arch_dir: &str) {
    let dir = root.join("vst3").join(id);
    fs::create_dir_all(dir.join("bundles/Test.vst3/Contents").join(arch_dir)).unwrap();
    fs::write(
        dir.join("manifest.yaml"),
        format!(
            "manifest_version: 1\nid: {id}\ndisplay_name: {id}\ntype: vst3\n\
             backend: vst3\nbundle: bundles/Test.vst3\nparameters: []\n"
        ),
    )
    .unwrap();
}

fn available(model_id: &str) -> bool {
    plugin_loader::registry::model_available(model_id, |_| false, |_| true)
}

#[test]
fn packages_without_a_host_binary_are_registered_but_not_available() {
    // Per clone (CARGO_TARGET_TMPDIR), so parallel runs in other .solvers
    // checkouts cannot delete these fixtures mid-scan.
    let root =
        std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("issue978_foreign_platform");
    let _ = fs::remove_dir_all(&root);
    let (host_slot, host_vst3) = host();
    let (foreign_slot, foreign_vst3) = foreign();
    write_lv2(&root, "lv2_host", host_slot);
    write_lv2(&root, "lv2_foreign", foreign_slot);
    write_vst3(&root, "vst3_host", host_vst3);
    write_vst3(&root, "vst3_foreign", foreign_vst3);

    plugin_loader::registry::init_many(&[root.clone()]);

    for id in ["lv2_host", "lv2_foreign", "vst3_host", "vst3_foreign"] {
        assert!(
            plugin_loader::registry::find(id).is_some(),
            "{id} must still load: a missing slot is not corruption (#477)"
        );
    }
    assert!(available("lv2_host"));
    assert!(available("vst3_host"));
    assert!(
        !available("lv2_foreign"),
        "an LV2 package with no binary for this platform cannot be built here"
    );
    assert!(
        !available("vst3_foreign"),
        "a VST3 bundle with no binary for this platform cannot be built here"
    );

    let _ = fs::remove_dir_all(&root);
}
