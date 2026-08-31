//! #913 — applying a chosen folder to the running session.
//!
//! Two things happen and BOTH must: the override is persisted into
//! `config.yaml`, and the shared in-memory `AppConfig` is mirrored to match.
//! #607 is exactly the second half — with only the disk write, the next
//! lifecycle save re-persisted the stale in-memory snapshot and clobbered the
//! user's pick back to its startup value.
//!
//! The `_at` variants exist so this runs against a throwaway config file; the
//! machine's real one is never touched (#701).

use super::paths_apply::{
    apply_evaluations_path_at, apply_plugins_path_at, apply_presets_path_at,
    run_reload_plugin_catalog,
};
use crate::state::ProjectSession;
use infra_filesystem::{AppConfig, FilesystemStorage};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn config_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.yaml");
    (dir, path)
}

fn no_session() -> Rc<RefCell<Option<ProjectSession>>> {
    Rc::new(RefCell::new(None))
}

fn shared_config() -> Rc<RefCell<AppConfig>> {
    Rc::new(RefCell::new(AppConfig::default()))
}

#[test]
fn choosing_a_presets_folder_persists_it_and_mirrors_the_shared_snapshot() {
    let (_guard, config_path) = config_file();
    let app_config = shared_config();
    let chosen = PathBuf::from("/tmp/openrig-913-presets");

    apply_presets_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(chosen.clone()),
    );

    assert_eq!(
        FilesystemStorage::load_app_config_at(&config_path)
            .expect("load")
            .paths
            .presets_path,
        Some(chosen.clone()),
        "the pick must reach config.yaml"
    );
    assert_eq!(
        app_config.borrow().paths.presets_path,
        Some(chosen),
        "#607: without the in-memory mirror the next whole-config save undoes the pick"
    );
}

#[test]
fn choosing_a_plugins_folder_persists_it_and_mirrors_the_shared_snapshot() {
    let (_guard, config_path) = config_file();
    let app_config = shared_config();
    let chosen = PathBuf::from("/tmp/openrig-913-plugins");
    apply_plugins_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(chosen.clone()),
    );
    assert_eq!(
        FilesystemStorage::load_app_config_at(&config_path)
            .expect("load")
            .paths
            .plugins_path,
        Some(chosen.clone())
    );
    assert_eq!(app_config.borrow().paths.plugins_path, Some(chosen));
}

#[test]
fn choosing_an_evaluations_folder_persists_it_and_mirrors_the_shared_snapshot() {
    let (_guard, config_path) = config_file();
    let app_config = shared_config();
    let chosen = PathBuf::from("/tmp/openrig-913-evaluations");
    apply_evaluations_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(chosen.clone()),
    );
    assert_eq!(
        FilesystemStorage::load_app_config_at(&config_path)
            .expect("load")
            .paths
            .evaluations_path,
        Some(chosen.clone())
    );
    assert_eq!(app_config.borrow().paths.evaluations_path, Some(chosen));
}

#[test]
fn clearing_an_override_resets_it_so_the_os_default_wins_again() {
    let (_guard, config_path) = config_file();
    let app_config = shared_config();
    apply_presets_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(PathBuf::from("/tmp/openrig-913-presets")),
    );
    apply_presets_path_at(&config_path, &no_session(), &app_config, None);
    assert_eq!(
        FilesystemStorage::load_app_config_at(&config_path)
            .expect("load")
            .paths
            .presets_path,
        None
    );
    assert_eq!(app_config.borrow().paths.presets_path, None);
}

#[test]
fn each_override_writes_only_its_own_field() {
    let (_guard, config_path) = config_file();
    let app_config = shared_config();
    apply_presets_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(PathBuf::from("/tmp/p")),
    );
    apply_plugins_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(PathBuf::from("/tmp/g")),
    );
    let on_disk = FilesystemStorage::load_app_config_at(&config_path).expect("load");
    assert_eq!(on_disk.paths.presets_path, Some(PathBuf::from("/tmp/p")));
    assert_eq!(on_disk.paths.plugins_path, Some(PathBuf::from("/tmp/g")));
    assert_eq!(on_disk.paths.evaluations_path, None);
}

#[test]
fn applying_a_folder_with_no_project_open_persists_without_dispatching() {
    // The settings screen is reachable from the launcher, before any project
    // exists. The persist must still happen; there is simply no bus to fan out on.
    let (_guard, config_path) = config_file();
    let app_config = shared_config();
    apply_presets_path_at(
        &config_path,
        &no_session(),
        &app_config,
        Some(PathBuf::from("/tmp/openrig-913-nosession")),
    );
    assert_eq!(
        app_config.borrow().paths.presets_path,
        Some(PathBuf::from("/tmp/openrig-913-nosession"))
    );
}

#[test]
fn reloading_the_catalog_without_a_project_still_reports_the_totals() {
    // The catalog is process-wide, not project-scoped: the reload runs through
    // a throwaway dispatcher and must come back with a summary the status line
    // can show, never an empty string.
    let summary = run_reload_plugin_catalog(&no_session());
    assert!(!summary.is_empty());
    assert!(
        summary.contains("plugin") || summary.contains("reload"),
        "unexpected status text: {summary}"
    );
}
