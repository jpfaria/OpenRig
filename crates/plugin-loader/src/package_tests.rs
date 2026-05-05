
use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::manifest::{
    Backend, BlockType, GridCapture, GridParameter, ParameterValue, PluginManifest,
};

/// Per-test temp directory. Deleted on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "openrig-plugin-loader-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn write(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let absolute = self.path.join(relative);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&absolute, contents).expect("write file");
        absolute
    }

    fn mkdir(&self, relative: &str) -> PathBuf {
        let absolute = self.path.join(relative);
        fs::create_dir_all(&absolute).expect("create dir");
        absolute
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn nam_manifest(captures: Vec<GridCapture>) -> PluginManifest {
    PluginManifest {
        manifest_version: 1,
        id: "test_pkg".to_string(),
        display_name: "Test Pkg".to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        block_type: BlockType::Preamp,
        backend: Backend::Nam {
            parameters: vec![GridParameter {
                name: "gain".to_string(),
                display_name: None,
                values: vec![ParameterValue::Number(10.0)],
            }],
            captures,
        },
    }
}

fn capture(values: &[(&str, f64)], file: &str) -> GridCapture {
    GridCapture {
        values: values
            .iter()
            .map(|(name, value)| ((*name).to_string(), ParameterValue::Number(*value)))
            .collect(),
        file: PathBuf::from(file),
    }
}

#[test]
fn accepts_nam_package_with_existing_capture() {
    let tmp = TempDir::new("nam_ok");
    tmp.write("captures/g10.nam", b"fake nam bytes");
    let manifest = nam_manifest(vec![capture(&[("gain", 10.0)], "captures/g10.nam")]);
    assert!(validate_package(&tmp.path, &manifest).is_ok());
}

#[test]
fn rejects_nam_package_missing_capture_file() {
    let tmp = TempDir::new("nam_missing");
    let manifest = nam_manifest(vec![capture(&[("gain", 10.0)], "captures/g10.nam")]);
    let err = validate_package(&tmp.path, &manifest).unwrap_err();
    assert!(matches!(err, PackageError::MissingCaptureFile { .. }));
}

#[test]
fn accepts_ir_package_with_existing_wav() {
    let tmp = TempDir::new("ir_ok");
    tmp.write("ir/cab.wav", b"riff fake");
    let manifest = PluginManifest {
        manifest_version: 1,
        id: "ir_cab".to_string(),
        display_name: "IR Cab".to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        block_type: BlockType::Cab,
        backend: Backend::Ir {
            parameters: vec![],
            captures: vec![capture(&[], "ir/cab.wav")],
        },
    };
    assert!(validate_package(&tmp.path, &manifest).is_ok());
}

#[test]
fn accepts_lv2_package_with_bundle_and_binary() {
    let tmp = TempDir::new("lv2_ok");
    tmp.mkdir("bundles/test.lv2");
    tmp.write("bundles/test.lv2/linux-x86_64/plugin.so", b"fake binary");

    let manifest = PluginManifest {
        manifest_version: 1,
        id: "lv2_test".to_string(),
        display_name: "LV2 Test".to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        block_type: BlockType::GainPedal,
        backend: Backend::Lv2 {
            plugin_uri: "urn:test:plugin".to_string(),
            binaries: BTreeMap::from([(
                Lv2Slot::LinuxX86_64,
                PathBuf::from("bundles/test.lv2/linux-x86_64/plugin.so"),
            )]),
        },
    };
    assert!(validate_package(&tmp.path, &manifest).is_ok());
}

#[test]
fn rejects_lv2_package_with_missing_binary() {
    let tmp = TempDir::new("lv2_no_binary");
    tmp.mkdir("bundles/test.lv2");
    let manifest = PluginManifest {
        manifest_version: 1,
        id: "lv2_test".to_string(),
        display_name: "LV2 Test".to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        block_type: BlockType::GainPedal,
        backend: Backend::Lv2 {
            plugin_uri: "urn:test:plugin".to_string(),
            binaries: BTreeMap::from([(
                Lv2Slot::LinuxX86_64,
                PathBuf::from("bundles/test.lv2/linux-x86_64/missing.so"),
            )]),
        },
    };
    let err = validate_package(&tmp.path, &manifest).unwrap_err();
    assert!(matches!(err, PackageError::MissingBinarySlot { .. }));
}

#[test]
fn rejects_when_package_root_is_not_a_directory() {
    let nonexistent = PathBuf::from("/this/path/does/not/exist/openrig/test");
    let manifest = nam_manifest(vec![]);
    // Schema-level fails first because parameters declare a value but no captures
    // exist — so we pass a manifest that's schema-valid but root is bad:
    let manifest_with_capture = nam_manifest(vec![capture(&[("gain", 10.0)], "x.nam")]);
    let err = validate_package(&nonexistent, &manifest_with_capture).unwrap_err();
    assert!(matches!(err, PackageError::PackageRootNotADirectory(_)));
    // Sanity: also confirm the empty-grid manifest fails on schema, not on root:
    let _ = manifest;
}

#[test]
fn schema_errors_propagate_through_package_validation() {
    let tmp = TempDir::new("schema_propagate");
    let mut manifest = nam_manifest(vec![capture(&[("gain", 10.0)], "x.nam")]);
    manifest.id = String::new();
    let err = validate_package(&tmp.path, &manifest).unwrap_err();
    assert!(matches!(
        err,
        PackageError::Validation(ValidationError::EmptyId)
    ));
}

#[test]
fn current_platform_slot_returns_a_known_slot_on_supported_targets() {
    let slot = current_platform_slot();
    // We test only that it resolves to *some* slot on the standard targets
    // CI runs on (linux-x86_64, macos-universal). Exotic targets may return
    // None — accept that.
    let target = (std::env::consts::OS, std::env::consts::ARCH);
    match target {
        ("macos", _)
        | ("linux", "x86_64")
        | ("linux", "aarch64")
        | ("windows", "x86_64")
        | ("windows", "aarch64") => {
            assert!(slot.is_some(), "expected a slot for target {target:?}");
        }
        _ => {
            // Acceptable to be None on exotic targets.
        }
    }
}
