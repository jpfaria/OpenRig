//! Responsibility: decides where this session reads its project from.

use crate::project_ops_recents::parse_path_argument;
use crate::state::ProjectPaths;
use std::env;
use std::path::PathBuf;

pub(crate) fn resolve_project_paths() -> ProjectPaths {
    ProjectPaths {
        default_config_path: parse_path_argument("--config").unwrap_or_else(|| {
            let local = PathBuf::from("config.yaml");
            if local.exists() {
                local
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.yaml")
            }
        }),
    }
}
