use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "openrig-install-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_test_zip(zip_path: &std::path::Path) {
    let file = File::create(zip_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("nam/alpha/manifest.yaml", opts).unwrap();
    writer.write_all(b"id: nam_alpha\n").unwrap();
    writer.start_file("nam/alpha/captures/g10.nam", opts).unwrap();
    writer.write_all(b"fake nam bytes").unwrap();
    writer.finish().unwrap();
}

#[test]
fn extracts_when_destination_empty() {
    let tmp = TempDir::new("extracts");
    let zip = tmp.path.join("bundle.zip");
    let dest = tmp.path.join("plugins");
    write_test_zip(&zip);

    let written = extract_bundle_if_needed(&dest, &zip).unwrap();
    assert_eq!(written, 2);
    assert!(dest.join("nam/alpha/manifest.yaml").is_file());
    assert!(dest.join("nam/alpha/captures/g10.nam").is_file());
}

#[test]
fn skips_when_destination_has_packages() {
    let tmp = TempDir::new("skips");
    let zip = tmp.path.join("bundle.zip");
    let dest = tmp.path.join("plugins");
    write_test_zip(&zip);
    fs::create_dir_all(dest.join("ir/some_pkg")).unwrap();
    fs::write(dest.join("ir/some_pkg/manifest.yaml"), b"id: x").unwrap();

    let written = extract_bundle_if_needed(&dest, &zip).unwrap();
    assert_eq!(written, 0);
    // The pre-existing package was NOT touched.
    assert!(dest.join("ir/some_pkg/manifest.yaml").is_file());
    // The zip's content was NOT extracted on top.
    assert!(!dest.join("nam/alpha/manifest.yaml").exists());
}

#[test]
fn skips_when_zip_missing() {
    let tmp = TempDir::new("missing");
    let dest = tmp.path.join("plugins");
    let zip = tmp.path.join("does-not-exist.zip");

    let written = extract_bundle_if_needed(&dest, &zip).unwrap();
    assert_eq!(written, 0);
    assert!(!dest.exists());
}

#[test]
fn has_extracted_packages_detects_real_layout() {
    let tmp = TempDir::new("detect");
    let dest = tmp.path.join("plugins");
    fs::create_dir_all(dest.join("lv2/some_id")).unwrap();
    fs::write(dest.join("lv2/some_id/manifest.yaml"), b"x").unwrap();
    assert!(has_extracted_packages(&dest));
}

#[test]
fn has_extracted_packages_returns_false_for_empty() {
    let tmp = TempDir::new("detect_empty");
    let dest = tmp.path.join("plugins");
    fs::create_dir_all(dest.join("lv2")).unwrap();
    assert!(!has_extracted_packages(&dest));
}
