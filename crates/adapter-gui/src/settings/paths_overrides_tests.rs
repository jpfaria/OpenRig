//! #913 — persisting a path override and mirroring it in memory.
//!
//! #607: the disk write alone is not enough. Lifecycle events re-persist the
//! whole in-memory `AppConfig`, so an override that only reached disk was
//! clobbered back to its startup value on the next project open. Both halves
//! happen here, or neither.

use super::paths_overrides::{
    apply_evaluations_override_at, apply_plugins_override_at, apply_presets_override_at,
};
use infra_filesystem::{AppConfig, FilesystemStorage};
use std::path::PathBuf;

fn config_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    (dir, path)
}

#[test]
fn each_override_lands_on_disk_and_in_the_shared_snapshot() {
    let (_guard, path) = config_file();
    let mut config = AppConfig::default();

    apply_presets_override_at(&path, &mut config, Some(PathBuf::from("/tmp/p"))).expect("presets");
    apply_plugins_override_at(&path, &mut config, Some(PathBuf::from("/tmp/g"))).expect("plugins");
    apply_evaluations_override_at(&path, &mut config, Some(PathBuf::from("/tmp/e")))
        .expect("evaluations");

    assert_eq!(config.paths.presets_path, Some(PathBuf::from("/tmp/p")));
    assert_eq!(config.paths.plugins_path, Some(PathBuf::from("/tmp/g")));
    assert_eq!(config.paths.evaluations_path, Some(PathBuf::from("/tmp/e")));

    let on_disk = FilesystemStorage::load_app_config_at(&path).expect("load");
    assert_eq!(on_disk.paths.presets_path, Some(PathBuf::from("/tmp/p")));
    assert_eq!(on_disk.paths.plugins_path, Some(PathBuf::from("/tmp/g")));
    assert_eq!(
        on_disk.paths.evaluations_path,
        Some(PathBuf::from("/tmp/e"))
    );
}

#[test]
fn an_override_preserves_the_bindings_already_in_the_file() {
    let (_guard, path) = config_file();
    let mut seeded = AppConfig::default();
    seeded.paths.plugins_path = Some(PathBuf::from("/tmp/keep-me"));
    FilesystemStorage::save_app_config_at(&path, &seeded).expect("seed");

    let mut config = AppConfig::default();
    apply_presets_override_at(&path, &mut config, Some(PathBuf::from("/tmp/p"))).expect("presets");

    assert_eq!(
        FilesystemStorage::load_app_config_at(&path)
            .expect("load")
            .paths
            .plugins_path,
        Some(PathBuf::from("/tmp/keep-me")),
        "writing one override must not rewrite the document from defaults"
    );
}

#[test]
fn clearing_an_override_removes_it_from_both_places() {
    let (_guard, path) = config_file();
    let mut config = AppConfig::default();
    apply_presets_override_at(&path, &mut config, Some(PathBuf::from("/tmp/p"))).expect("set");
    apply_presets_override_at(&path, &mut config, None).expect("clear");
    assert_eq!(config.paths.presets_path, None);
    assert_eq!(
        FilesystemStorage::load_app_config_at(&path)
            .expect("load")
            .paths
            .presets_path,
        None
    );
}
