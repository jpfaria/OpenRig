//! #913 — the plugin catalog the app starts with.
//!
//! Boot scans two roots and registers the compiled-in natives first, so by the
//! time any project opens everything lives in one catalog. What must hold: the
//! natives are always there (they need no disk at all, so a missing or empty
//! plugins directory can never leave the app with no blocks), and loading is
//! idempotent — the settings screen re-runs it on "reload catalog".

use super::load;
use crate::state::ProjectPaths;

fn paths_pointing_nowhere() -> (tempfile::TempDir, ProjectPaths) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = ProjectPaths {
        default_config_path: dir.path().join("config.yaml"),
    };
    (dir, paths)
}

#[test]
fn the_compiled_in_natives_are_registered_even_with_no_plugins_on_disk() {
    let (_guard, paths) = paths_pointing_nowhere();
    load(&paths, 48_000.0);
    assert!(
        plugin_loader::registry::native_count() > 0,
        "the natives need no disk — an empty plugins dir must still leave blocks"
    );
    assert!(plugin_loader::registry::len() >= plugin_loader::registry::native_count());
}

#[test]
fn loading_twice_does_not_duplicate_the_catalog() {
    let (_guard, paths) = paths_pointing_nowhere();
    load(&paths, 48_000.0);
    let after_first = plugin_loader::registry::len();
    load(&paths, 48_000.0);
    assert_eq!(
        plugin_loader::registry::len(),
        after_first,
        "the settings screen re-runs this; a second scan must not double the list"
    );
}
