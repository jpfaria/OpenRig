//! Issue #599 — red-first guard: the plugin catalog ignored the user's
//! plugins directory, leaving IR/NAM/LV2 pickers empty.
//!
//! Settings -> Paths (#513) saves the plugins directory under
//! `paths.plugins_path` in `config.yaml`. The loader's
//! `plugins_root_from_config` (#287) only read the top-level `plugins_root`
//! key, so the user's path was never scanned and the catalog fell back to
//! `<config_dir>/plugins` (empty) -> zero disk packages.
//!
//! These tests pin the resolution contract: env override, then the
//! canonical `paths.plugins_path`, then the legacy `plugins_root`, then the
//! install-layout fallback.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use plugin_loader::plugins_root_from_config;

/// Throwaway temp directory, removed on drop. Mirrors the helper in
/// `src/install_tests.rs` (no `tempfile` dev-dependency in this crate).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "openrig-config-{label}-{}-{unique}",
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

fn write_config(dir: &Path, body: &str) -> PathBuf {
    let path = dir.join("config.yaml");
    fs::write(&path, body).expect("write config.yaml");
    path
}

#[test]
fn reads_paths_plugins_path() {
    // Real configs (Settings -> Paths, #513) store the plugins directory
    // under `paths.plugins_path`, NOT the top-level `plugins_root`.
    let tmp = TempDir::new("paths-plugins-path");
    let plugins_dir = tmp.path.join("my-plugins");
    fs::create_dir_all(&plugins_dir).expect("create plugins dir");
    let config_path = write_config(
        &tmp.path,
        &format!("paths:\n  plugins_path: {}\n", plugins_dir.display()),
    );

    let resolved = plugins_root_from_config(&config_path);

    assert_eq!(
        resolved, plugins_dir,
        "expected loader to resolve paths.plugins_path, got {resolved:?}"
    );
}

#[test]
fn prefers_paths_plugins_path_over_legacy_plugins_root() {
    // Precedence: the canonical `paths.plugins_path` (#513) wins over the
    // legacy top-level `plugins_root` (#287) when a config carries both.
    let tmp = TempDir::new("precedence");
    let canonical = tmp.path.join("canonical");
    let legacy = tmp.path.join("legacy");
    fs::create_dir_all(&canonical).expect("create canonical dir");
    fs::create_dir_all(&legacy).expect("create legacy dir");
    let config_path = write_config(
        &tmp.path,
        &format!(
            "plugins_root: {}\npaths:\n  plugins_path: {}\n",
            legacy.display(),
            canonical.display()
        ),
    );

    let resolved = plugins_root_from_config(&config_path);

    assert_eq!(
        resolved, canonical,
        "paths.plugins_path must win over legacy plugins_root, got {resolved:?}"
    );
}

#[test]
fn honors_legacy_top_level_plugins_root() {
    // Back-compat guard: older configs that only carry the top-level
    // `plugins_root` (#287, no `paths` section) must keep working.
    let tmp = TempDir::new("legacy-only");
    let legacy = tmp.path.join("legacy-plugins");
    fs::create_dir_all(&legacy).expect("create legacy dir");
    let config_path = write_config(&tmp.path, &format!("plugins_root: {}\n", legacy.display()));

    let resolved = plugins_root_from_config(&config_path);

    assert_eq!(
        resolved, legacy,
        "legacy top-level plugins_root must still resolve, got {resolved:?}"
    );
}

#[test]
fn falls_back_to_config_dir_plugins() {
    // No env, no `paths.plugins_path`, no legacy `plugins_root` -> the
    // last-resort `<config_dir>/plugins` (install-layout default).
    let tmp = TempDir::new("fallback");
    let config_path = write_config(&tmp.path, "paths: {}\n");

    let resolved = plugins_root_from_config(&config_path);

    assert_eq!(
        resolved,
        tmp.path.join("plugins"),
        "expected <config_dir>/plugins fallback, got {resolved:?}"
    );
}
