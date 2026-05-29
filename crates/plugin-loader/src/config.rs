//! Resolves where the plugin packages directory lives at runtime.
//!
//! Order of precedence:
//!   1. `OPENRIG_PLUGINS_ROOT` environment variable.
//!   2. `paths.plugins_path:` in `config.yaml` — the canonical location
//!      written by the Settings -> Paths screen (#513).
//!   3. Legacy top-level `plugins_root:` in `config.yaml` (#287), kept for
//!      back-compat with configs that predate the Settings -> Paths screen.
//!   4. Last-resort default `<config dir>/plugins` — picks up whatever
//!      `plugins/` directory ships next to the config in the current
//!      install layout. The cross-platform layout (.app bundle / .deb /
//!      .msi) all place a `plugins/` directory next to the deployed
//!      `config.yaml`, so this works without per-OS hardcoding.
//!
//! For (2) and (3): relative paths resolve against the config file's
//! directory; absolute paths pass through unchanged.
//!
//! Issues: #287, #513, #599

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The `paths:` section of `config.yaml`, narrowed to the one field the
/// plugin loader reads. Mirrors the canonical owner in `infra-filesystem`'s
/// `AssetPaths` (a layering-safe read-only view — the loader must not depend
/// on `infra-filesystem`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginPathsSection {
    /// Directory containing every plugin package
    /// (`<plugins_path>/<backend>/<id>/manifest.yaml`). Written by the
    /// Settings -> Paths screen (#513).
    #[serde(default)]
    pub plugins_path: Option<PathBuf>,
}

/// Subset of `config.yaml` the plugin loader cares about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginPathsConfig {
    /// Canonical plugins directory, under `paths.plugins_path` (#513).
    #[serde(default)]
    pub paths: PluginPathsSection,
    /// Legacy top-level `plugins_root` (#287). Kept for back-compat with
    /// configs written before the Settings -> Paths screen existed.
    #[serde(default)]
    pub plugins_root: Option<PathBuf>,
}

/// Resolve the plugin packages directory to use at runtime.
///
/// `config_path` should point at the project's `config.yaml`. Precedence:
/// `OPENRIG_PLUGINS_ROOT` env wins outright; then `paths.plugins_path`
/// (canonical, #513); then the legacy top-level `plugins_root` (#287); then
/// the `<config_dir>/plugins` install-layout fallback. Relative declared
/// paths resolve against the config file's directory; absolute paths pass
/// through.
pub fn plugins_root_from_config(config_path: &Path) -> PathBuf {
    if let Ok(env_value) = std::env::var("OPENRIG_PLUGINS_ROOT") {
        let trimmed = env_value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if let Ok(yaml) = std::fs::read_to_string(config_path) {
        if let Ok(parsed) = serde_yaml::from_str::<PluginPathsConfig>(&yaml) {
            // Canonical `paths.plugins_path` (#513) wins over the legacy
            // top-level `plugins_root` (#287) when both are present.
            if let Some(declared) = parsed.paths.plugins_path.or(parsed.plugins_root) {
                return if declared.is_absolute() {
                    declared
                } else {
                    config_dir.join(declared)
                };
            }
        }
    }
    config_dir.join("plugins")
}
