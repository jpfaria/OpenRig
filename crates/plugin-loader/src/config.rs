//! Resolves where the plugin packages directory lives at runtime.
//!
//! Order of precedence:
//!   1. `OPENRIG_PLUGINS_ROOT` environment variable.
//!   2. `plugins_root:` field in the project-root `config.yaml`.
//!   3. Sibling-cloned default `../OpenRig-plugins/plugins/source` (the
//!      layout used during issue #287 development).
//!
//! The config.yaml shape this reads from looks like:
//!
//! ```yaml
//! plugins_root: ../OpenRig-plugins/plugins/bundle
//! ```
//!
//! Older `paths:` entries (lv2_libs, nam_captures, ir_captures, lv2_data)
//! aren't read here — they belong to the legacy resolve-by-asset-type
//! path that the runtime loader replaces. Issue #287 will retire those
//! entries once the engine has fully switched to package-based loading.
//!
//! Issue: #287

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Subset of `config.yaml` the plugin loader cares about. Other fields
/// (e.g. legacy `paths`) round-trip via `serde(flatten)` on the engine's
/// own config struct, not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginPathsConfig {
    /// Directory containing every bundled plugin package
    /// (`<plugins_root>/<backend>/<id>/manifest.yaml`). Resolved relative
    /// to the config file's directory when loaded via
    /// [`plugins_root_from_config`]; absolute paths pass through.
    #[serde(default)]
    pub plugins_root: Option<PathBuf>,
}

/// Resolve the plugin packages directory to use at runtime.
///
/// `config_path` should point at the project's `config.yaml`. When
/// `OPENRIG_PLUGINS_ROOT` is set in the environment, that wins outright.
/// Otherwise the file's `plugins_root` field is honored. If neither is
/// present, falls back to the sibling layout used during issue #287
/// development: `<config dir>/../OpenRig-plugins/plugins/source`.
/// (`plugins/bundle/` was the staged identity-copy output of the
/// optional build_plugin_bundle step; the engine now reads
/// `plugins/source/` directly since the build step adds no
/// transformation yet.)
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
            if let Some(declared) = parsed.plugins_root {
                let candidate = if declared.is_absolute() {
                    declared
                } else {
                    config_dir.join(declared)
                };
                return candidate;
            }
        }
    }
    config_dir.join("../OpenRig-plugins/plugins/source")
}
