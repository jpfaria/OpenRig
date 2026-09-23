//! Responsibility: proves the session's config falls back to the app's own config file
//!
//! #968 — the Settings → Paths screen writes `paths.plugins_path` into the OS
//! app config (`FilesystemStorage::app_config_path()`), but startup resolved
//! its config as `--config` → `./config.yaml` → `$CARGO_MANIFEST_DIR/../../
//! config.yaml`. Neither checkout ships a repo-root `config.yaml`, so the
//! resolved file did not exist, `plugins_root_from_config` fell through to
//! `<repo>/plugins`, and the app started with an empty plugin catalog —
//! working only when `OPENRIG_PLUGINS_ROOT` was exported by hand.
//!
//! The `CARGO_MANIFEST_DIR` fallback was also a hardcoded build-machine path,
//! meaningless inside a shipped `.app`.

use std::path::{Path, PathBuf};

use crate::project_paths_resolve::resolve_config_path;

#[test]
fn an_explicit_argument_wins() {
    let arg = PathBuf::from("/tmp/explicit/config.yaml");
    assert_eq!(
        resolve_config_path(Some(arg.clone()), Path::new("/nowhere/config.yaml"), || {
            true
        }),
        arg
    );
}

#[test]
fn a_config_in_the_working_directory_wins_over_the_app_config() {
    assert_eq!(
        resolve_config_path(None, Path::new("/app/config.yaml"), || true),
        PathBuf::from("config.yaml"),
        "a local config.yaml is the dev override and still wins"
    );
}

#[test]
fn without_a_local_config_the_app_config_is_used() {
    let app = PathBuf::from("/Users/someone/Library/Application Support/OpenRig/config.yaml");
    assert_eq!(
        resolve_config_path(None, &app, || false),
        app,
        "REGRESSION #968: with no local config the session must read the app's \
         own config — the file Settings → Paths writes `plugins_path` into — \
         not a path derived from the build directory"
    );
}
