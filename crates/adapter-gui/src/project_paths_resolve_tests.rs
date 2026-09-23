//! #913 — where this session reads its config from.
//!
//! Three sources in order: an explicit `--config` argument, a `config.yaml` in
//! the working directory, and (#968) the app's own config file. Whichever
//! wins, the answer must always name a `config.yaml` — a resolver that returned
//! a directory (or an empty path) would make the launcher read nothing and
//! start with defaults, silently discarding the user's setup.

use super::resolve_project_paths;

#[test]
fn the_resolved_config_always_names_a_config_file() {
    let paths = resolve_project_paths();
    assert_eq!(
        paths
            .default_config_path
            .file_name()
            .and_then(|n| n.to_str()),
        Some("config.yaml"),
        "resolved: {}",
        paths.default_config_path.display()
    );
}

#[test]
fn the_resolved_config_is_stable_across_calls() {
    assert_eq!(
        resolve_project_paths().default_config_path,
        resolve_project_paths().default_config_path,
        "two reads in one session must not disagree about where config lives"
    );
}

#[test]
fn the_fallback_is_an_absolute_path_when_the_cwd_has_no_config() {
    let paths = resolve_project_paths();
    let resolved = paths.default_config_path;
    let cwd_local = std::path::Path::new("config.yaml");
    if cwd_local.exists() {
        assert_eq!(resolved, cwd_local, "a local config.yaml wins");
    } else {
        assert!(
            resolved.is_absolute(),
            "#968: with no local config the answer is the app's own config file, \
             an absolute per-OS path — not a bare filename: {}",
            resolved.display()
        );
    }
}
