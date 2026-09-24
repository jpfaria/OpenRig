use std::fs;

use super::windows_module_binary;

const ARCH: &str = "x86_64-win";

#[test]
fn bundle_dll_named_after_the_bundle() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("Reverb.vst3/Contents").join(ARCH);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("Reverb.vst3"), b"dll").unwrap();

    assert_eq!(
        windows_module_binary(&root.path().join("Reverb.vst3"), ARCH),
        Some(dir.join("Reverb.vst3"))
    );
}

#[test]
fn renamed_bundle_still_finds_its_dll() {
    // The user renamed the folder; the DLL inside kept the vendor's name.
    // macOS and Linux already fall back to any binary in the arch dir.
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("My Reverb.vst3/Contents").join(ARCH);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("VendorReverb.vst3"), b"dll").unwrap();

    assert_eq!(
        windows_module_binary(&root.path().join("My Reverb.vst3"), ARCH),
        Some(dir.join("VendorReverb.vst3"))
    );
}

#[test]
fn single_file_module_is_its_own_dll() {
    // Pre-3.6.10 layout, still common on Windows: the .vst3 IS the DLL.
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("OldDelay.vst3");
    fs::write(&file, b"dll").unwrap();

    assert_eq!(windows_module_binary(&file, ARCH), Some(file));
}

#[test]
fn bundle_without_a_dll_for_this_arch_has_none() {
    let root = tempfile::tempdir().unwrap();
    let bundle = root.path().join("MacOnly.vst3");
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    fs::write(bundle.join("Contents/MacOS/MacOnly"), b"mach-o").unwrap();

    assert_eq!(windows_module_binary(&bundle, ARCH), None);
}
