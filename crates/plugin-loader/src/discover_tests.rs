use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// Per-test temp directory. Deleted on drop. Same shape as the helper in
/// `package::tests` — tiny enough to not warrant sharing yet.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "openrig-discover-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn write(&self, relative: &str, contents: &[u8]) {
        let absolute = self.path.join(relative);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&absolute, contents).expect("write file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_valid_nam_package(tmp: &TempDir, package_id: &str) {
    let manifest = format!(
        r#"manifest_version: 1
id: {package_id}
display_name: {package_id}
type: preamp
backend: nam
parameters:
  - name: gain
    values: [10]
captures:
  - values: {{ gain: 10 }}
    file: captures/g10.nam
"#,
    );
    tmp.write(&format!("{package_id}/manifest.yaml"), manifest.as_bytes());
    tmp.write(&format!("{package_id}/captures/g10.nam"), b"fake nam bytes");
}

#[test]
fn discovers_zero_packages_in_empty_directory() {
    let tmp = TempDir::new("empty");
    let results = discover(&tmp.path).expect("read dir");
    assert!(results.is_empty());
}

#[test]
fn discovers_two_valid_packages() {
    let tmp = TempDir::new("two_valid");
    write_valid_nam_package(&tmp, "alpha");
    write_valid_nam_package(&tmp, "beta");

    let results = discover(&tmp.path).expect("read dir");

    assert_eq!(results.len(), 2);
    let ids: Vec<String> = results
        .iter()
        .map(|result| result.as_ref().expect("valid package").manifest.id.clone())
        .collect();
    assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
}

#[test]
fn skips_subdirectory_without_manifest() {
    let tmp = TempDir::new("skip_no_manifest");
    write_valid_nam_package(&tmp, "alpha");
    // Stray directory that isn't a package — no manifest.yaml.
    fs::create_dir_all(tmp.path.join("not_a_package")).unwrap();
    fs::write(tmp.path.join("not_a_package/random.txt"), b"unrelated").unwrap();

    let results = discover(&tmp.path).expect("read dir");
    assert_eq!(results.len(), 1);
}

#[test]
fn skips_loose_files_at_top_level() {
    let tmp = TempDir::new("skip_loose");
    write_valid_nam_package(&tmp, "alpha");
    tmp.write("README.md", b"# bundle root readme");

    let results = discover(&tmp.path).expect("read dir");
    assert_eq!(results.len(), 1);
}

#[test]
fn collects_invalid_package_alongside_valid_one() {
    let tmp = TempDir::new("mixed");
    write_valid_nam_package(&tmp, "good");
    // A directory with a manifest.yaml that fails to parse.
    tmp.write("bad/manifest.yaml", b"not: valid: yaml: tree");

    let results = discover(&tmp.path).expect("read dir");
    assert_eq!(results.len(), 2);

    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    let err_count = results.iter().filter(|r| r.is_err()).count();
    assert_eq!(ok_count, 1);
    assert_eq!(err_count, 1);

    let err = results
        .iter()
        .find_map(|r| r.as_ref().err())
        .expect("at least one error");
    assert!(matches!(err, DiscoveryError::ManifestParse { .. }));
}

#[test]
fn surfaces_validation_errors_per_package() {
    let tmp = TempDir::new("validation_err");
    // Manifest parses but references a capture file that doesn't exist.
    tmp.write(
        "broken/manifest.yaml",
        br#"manifest_version: 1
id: broken
display_name: Broken
type: preamp
backend: nam
parameters:
  - name: gain
    values: [10]
captures:
  - values: { gain: 10 }
    file: captures/missing.nam
"#,
    );

    let results = discover(&tmp.path).expect("read dir");
    assert_eq!(results.len(), 1);
    let err = results[0].as_ref().expect_err("validation error");
    match err {
        DiscoveryError::Package { source, .. } => match source {
            PackageError::MissingCaptureFile { .. } => {}
            other => panic!("expected MissingCaptureFile, got {other:?}"),
        },
        other => panic!("expected Package error, got {other:?}"),
    }
}

#[test]
fn fails_when_bundle_root_is_missing() {
    let nonexistent = PathBuf::from("/nonexistent/openrig-discover-test-root");
    assert!(discover(&nonexistent).is_err());
}
