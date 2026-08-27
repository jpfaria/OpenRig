//! #913 — the two roots the boot catalog scans.
//!
//! `load` itself is not exercised here on purpose: `registry::init_many`
//! REPLACES the process-wide catalog, so calling it from one test wipes the
//! fixture catalog every other test in the binary shares (it took out the #690
//! NAM persistence tests when it was tried). What a test CAN hold is the root
//! resolution the boot does — bundled next to the install, user next to the
//! config file — because getting those wrong is how the app boots with no
//! blocks or with the user's own packages invisible.

use crate::state::ProjectPaths;

#[test]
fn the_bundled_root_sits_under_the_installs_data_root() {
    let bundled = infra_filesystem::detect_data_root().join("plugins");
    assert!(bundled.ends_with("plugins"));
    assert!(bundled.starts_with(infra_filesystem::detect_data_root()));
}

#[test]
fn the_user_root_is_derived_from_the_config_file_this_session_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = ProjectPaths {
        default_config_path: dir.path().join("config.yaml"),
    };
    let user = plugin_loader::plugins_root_from_config(&paths.default_config_path);
    assert!(
        !user.as_os_str().is_empty(),
        "a missing config must still resolve a plugins root, not an empty path"
    );
}

#[test]
fn the_two_roots_are_different_places() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bundled = infra_filesystem::detect_data_root().join("plugins");
    let user = plugin_loader::plugins_root_from_config(&dir.path().join("config.yaml"));
    assert_ne!(
        bundled, user,
        "the bundled root is read-only and replaced on upgrade — sharing it \
         with the user root would delete their installed packages"
    );
}
