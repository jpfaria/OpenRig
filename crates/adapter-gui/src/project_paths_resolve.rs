//! Responsibility: decides where this session reads its project from.

use crate::project_ops_recents::parse_path_argument;
use crate::state::ProjectPaths;
use std::path::{Path, PathBuf};

/// Where this session reads its config from, as a pure decision over the
/// three sources: an explicit `--config`, a `config.yaml` in the working
/// directory (the dev override), and `app_config` — the app's own file.
pub(crate) fn resolve_config_path(
    argument: Option<PathBuf>,
    app_config: &Path,
    local_config_exists: impl Fn() -> bool,
) -> PathBuf {
    if let Some(path) = argument {
        return path;
    }
    if local_config_exists() {
        return PathBuf::from("config.yaml");
    }
    // #968: the app's own config — the file Settings → Paths writes
    // `plugins_path` into. The old fallback pointed at the build directory, so
    // a checkout without a repo-root `config.yaml` started with no plugins.
    app_config.to_path_buf()
}

pub(crate) fn resolve_project_paths() -> ProjectPaths {
    ProjectPaths {
        default_config_path: resolve_config_path(
            parse_path_argument("--config"),
            &app_config_path(),
            || PathBuf::from("config.yaml").exists(),
        ),
    }
}

/// The app's own config file, the one Settings → Paths writes into.
fn app_config_path() -> PathBuf {
    infra_filesystem::FilesystemStorage::app_config_path()
        .unwrap_or_else(|_| PathBuf::from("config.yaml"))
}

#[cfg(test)]
#[path = "project_paths_resolve_tests.rs"]
mod tests;
