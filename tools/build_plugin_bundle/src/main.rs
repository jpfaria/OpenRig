//! Generates `plugins/bundle/` packages from `plugins/source/` assets.
//!
//! For now, the build is mostly a structural identity: each `plugins/source/<id>/`
//! that contains a valid `manifest.yaml` is validated and copied verbatim to
//! `plugins/bundle/<id>/`. Future iterations will add transformations
//! (resampling, normalization, repacking) on top of the same pipeline.
//!
//! Usage:
//!
//! ```text
//! build_plugin_bundle [--source <path>] [--bundle <path>]
//! ```
//!
//! Defaults are `plugins/source` and `plugins/bundle` relative to the current
//! working directory. Exits non-zero when any package fails to build.
//!
//! Issue: #287

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plugin_loader::{validate_package, PackageError, PluginManifest};

const DEFAULT_SOURCE_DIR: &str = "plugins/source";
const DEFAULT_BUNDLE_DIR: &str = "plugins/bundle";

fn main() -> ExitCode {
    match try_main() {
        Ok(report) => {
            print_report(&report);
            if report.failed.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("build_plugin_bundle: {error}");
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> anyhow::Result<BuildReport> {
    let args = parse_args(std::env::args().skip(1))?;
    let source = args
        .source
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOURCE_DIR));
    let bundle = args
        .bundle
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_BUNDLE_DIR));

    if !source.is_dir() {
        anyhow::bail!(
            "source directory `{}` does not exist (use --source <path> to override)",
            source.display()
        );
    }
    fs::create_dir_all(&bundle)?;

    Ok(build_bundle(&source, &bundle)?)
}

#[derive(Debug, Default)]
struct Args {
    source: Option<PathBuf>,
    bundle: Option<PathBuf>,
}

fn parse_args<I>(iter: I) -> anyhow::Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut args = Args::default();
    let mut iter = iter.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--source" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--source needs a path argument"))?;
                args.source = Some(PathBuf::from(value));
            }
            "--bundle" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--bundle needs a path argument"))?;
                args.bundle = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: build_plugin_bundle [--source <path>] [--bundle <path>]\n\
                     Defaults: --source {DEFAULT_SOURCE_DIR} --bundle {DEFAULT_BUNDLE_DIR}",
                );
                std::process::exit(0);
            }
            unknown => anyhow::bail!("unknown argument `{unknown}`"),
        }
    }
    Ok(args)
}

/// Outcome of a single build run.
#[derive(Debug, Default)]
pub struct BuildReport {
    pub built: Vec<String>,
    pub failed: Vec<BuildFailure>,
}

/// One package that failed to build.
#[derive(Debug)]
pub struct BuildFailure {
    pub source_path: PathBuf,
    pub error: BuildError,
}

/// Why a build failed for a given source package.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("failed to read manifest.yaml: {0}")]
    ReadManifest(#[source] io::Error),
    #[error("invalid manifest.yaml: {0}")]
    ParseManifest(#[source] serde_yaml::Error),
    #[error("validation failed: {0}")]
    Validation(#[source] PackageError),
    #[error("copy from source to bundle failed: {0}")]
    Copy(#[source] io::Error),
}

/// Build every package in `source_root` into `bundle_root`.
///
/// Each immediate sub-folder of `source_root` containing a `manifest.yaml`
/// is treated as a source package. For each, the entire sub-folder is
/// validated and then copied to `bundle_root/<package-id>/`. Existing
/// destination folders are replaced atomically per package: copy first into
/// a sibling temp folder, then rename over the old one.
pub fn build_bundle(source_root: &Path, bundle_root: &Path) -> io::Result<BuildReport> {
    let mut report = BuildReport::default();
    let mut entries: Vec<PathBuf> = Vec::new();
    walk_packages(source_root, &mut entries)?;
    entries.sort();

    for source_package in entries {
        let relative = source_package
            .strip_prefix(source_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| source_package.clone());
        match build_one_package(&source_package, bundle_root, &relative) {
            Ok(_) => {
                report.built.push(relative.to_string_lossy().to_string());
            }
            Err(error) => report.failed.push(BuildFailure {
                source_path: source_package,
                error,
            }),
        }
    }

    Ok(report)
}

/// Walk every sub-directory under `root` and collect those that hold a
/// `manifest.yaml`. Packages can live one level deep (legacy) or two
/// levels deep (`<root>/<backend>/<id>/manifest.yaml`).
fn walk_packages(root: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("manifest.yaml").is_file() {
            out.push(path);
        } else {
            walk_packages(&path, out)?;
        }
    }
    Ok(())
}

fn build_one_package(
    source_package: &Path,
    bundle_root: &Path,
    relative: &Path,
) -> Result<String, BuildError> {
    let manifest_path = source_package.join("manifest.yaml");
    let yaml = fs::read_to_string(&manifest_path).map_err(BuildError::ReadManifest)?;
    let manifest: PluginManifest =
        serde_yaml::from_str(&yaml).map_err(BuildError::ParseManifest)?;
    validate_package(source_package, &manifest).map_err(BuildError::Validation)?;

    // Bundle layout mirrors the source layout (`<backend>/<id>/`).
    let destination = bundle_root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(BuildError::Copy)?;
    }
    let staging_name = format!(
        ".staging.{}",
        relative.to_string_lossy().replace(['/', '\\'], "__")
    );
    let staging = bundle_root.join(staging_name);
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(BuildError::Copy)?;
    }
    copy_dir_recursive(source_package, &staging).map_err(BuildError::Copy)?;
    if destination.exists() {
        fs::remove_dir_all(&destination).map_err(BuildError::Copy)?;
    }
    fs::rename(&staging, &destination).map_err(BuildError::Copy)?;

    Ok(manifest.id)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_type = entry.file_type()?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if entry_type.is_file() {
            fs::copy(&from, &to)?;
        }
        // Symlinks are intentionally skipped — packages aren't expected to
        // contain them and following them blindly would be a footgun.
    }
    Ok(())
}

fn print_report(report: &BuildReport) {
    println!(
        "build_plugin_bundle: built {} package(s), {} failed",
        report.built.len(),
        report.failed.len()
    );
    for id in &report.built {
        println!("  ok    {id}");
    }
    for failure in &report.failed {
        let label = failure
            .source_path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("<unknown>");
        eprintln!("  FAIL  {label}: {}", failure.error);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
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
                "openrig-build-bundle-{label}-{}-{unique}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp");
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

    fn write_valid_nam_source(tmp: &TempDir, package_id: &str) {
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
        tmp.write(
            &format!("source/{package_id}/manifest.yaml"),
            manifest.as_bytes(),
        );
        tmp.write(
            &format!("source/{package_id}/captures/g10.nam"),
            b"fake nam bytes",
        );
    }

    #[test]
    fn builds_one_package_into_empty_bundle() {
        let tmp = TempDir::new("one_pkg");
        write_valid_nam_source(&tmp, "alpha");

        let source = tmp.path.join("source");
        let bundle = tmp.path.join("bundle");
        fs::create_dir_all(&bundle).unwrap();

        let report = build_bundle(&source, &bundle).expect("build");

        assert_eq!(report.built, vec!["alpha".to_string()]);
        assert!(report.failed.is_empty());
        assert!(bundle.join("alpha/manifest.yaml").is_file());
        assert!(bundle.join("alpha/captures/g10.nam").is_file());
    }

    #[test]
    fn build_is_idempotent_and_replaces_existing_package() {
        let tmp = TempDir::new("idempotent");
        write_valid_nam_source(&tmp, "alpha");

        let source = tmp.path.join("source");
        let bundle = tmp.path.join("bundle");
        fs::create_dir_all(&bundle).unwrap();

        let _ = build_bundle(&source, &bundle).expect("first build");
        // Drop a stale file inside the destination to confirm rebuild replaces it.
        fs::write(bundle.join("alpha/stale.txt"), b"leftover").unwrap();

        let report = build_bundle(&source, &bundle).expect("second build");

        assert_eq!(report.built, vec!["alpha".to_string()]);
        assert!(
            !bundle.join("alpha/stale.txt").exists(),
            "stale file removed"
        );
        assert!(bundle.join("alpha/manifest.yaml").is_file());
    }

    #[test]
    fn invalid_package_is_collected_and_does_not_block_others() {
        let tmp = TempDir::new("mixed");
        write_valid_nam_source(&tmp, "good");

        // A second package with a manifest that fails schema-level validation
        // (LV2 with an empty plugin_uri).
        tmp.write(
            "source/bad/manifest.yaml",
            br#"manifest_version: 1
id: bad
display_name: Bad
type: util
backend: lv2
plugin_uri: ""
bundle_path: bundles/bad.lv2
binaries:
  linux-x86_64: bundles/bad.lv2/linux-x86_64/plugin.so
"#,
        );

        let source = tmp.path.join("source");
        let bundle = tmp.path.join("bundle");
        fs::create_dir_all(&bundle).unwrap();

        let report = build_bundle(&source, &bundle).expect("build");

        assert_eq!(report.built, vec!["good".to_string()]);
        assert_eq!(report.failed.len(), 1);
        assert!(matches!(report.failed[0].error, BuildError::Validation(_)));
        assert!(bundle.join("good/manifest.yaml").is_file());
        assert!(!bundle.join("bad").exists());
    }

    #[test]
    fn copy_recursive_preserves_nested_layout() {
        let tmp = TempDir::new("nested");
        tmp.write("a/b/c/d/leaf.bin", b"deep content");
        let dest = tmp.path.join("copy");
        copy_dir_recursive(&tmp.path.join("a"), &dest).unwrap();
        assert!(dest.join("b/c/d/leaf.bin").is_file());
    }

    #[test]
    fn empty_source_yields_empty_report() {
        let tmp = TempDir::new("empty");
        let source = tmp.path.join("source");
        let bundle = tmp.path.join("bundle");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&bundle).unwrap();

        let report = build_bundle(&source, &bundle).expect("build");

        assert!(report.built.is_empty());
        assert!(report.failed.is_empty());
    }

    #[test]
    fn parse_args_accepts_source_and_bundle_overrides() {
        let args = parse_args(
            ["--source", "/tmp/src", "--bundle", "/tmp/bun"]
                .into_iter()
                .map(String::from),
        )
        .expect("parse");
        assert_eq!(args.source, Some(PathBuf::from("/tmp/src")));
        assert_eq!(args.bundle, Some(PathBuf::from("/tmp/bun")));
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        let result = parse_args(["--what".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn missing_capture_file_in_source_is_reported_as_validation_failure() {
        let tmp = TempDir::new("missing");
        // Manifest references a capture file that isn't on disk.
        tmp.write(
            "source/incomplete/manifest.yaml",
            br#"manifest_version: 1
id: incomplete
display_name: Incomplete
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

        let source = tmp.path.join("source");
        let bundle = tmp.path.join("bundle");
        fs::create_dir_all(&bundle).unwrap();

        let report = build_bundle(&source, &bundle).expect("build");

        assert!(report.built.is_empty());
        assert_eq!(report.failed.len(), 1);
        match &report.failed[0].error {
            BuildError::Validation(PackageError::MissingCaptureFile { .. }) => {}
            other => panic!("expected MissingCaptureFile, got {other:?}"),
        }
    }

    // Silence unused-import warning that crops up in tests when BTreeMap
    // is imported only conditionally — keeps `cargo build` clean.
    #[allow(dead_code)]
    fn _btreemap_marker() -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}
