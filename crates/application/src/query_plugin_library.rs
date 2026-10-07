//! Responsibility: serializes the plugin library for every transport.

use anyhow::{anyhow, Result};
use plugin_loader::version_store::version_numbers;
use project::block::grid_version_follow::capture_grid;

use crate::plugin_library::disk_manifest::read_disk_manifest;
use crate::plugin_library::entries::library_entries;
use crate::plugin_library::grid_read::read_grid;
use crate::plugin_library::PluginRoots;

/// Every plugin the user owns, as `{"plugins": [...]}`.
pub fn plugin_library_json(roots: &PluginRoots) -> String {
    serde_json::json!({ "plugins": library_entries(roots) }).to_string()
}

/// One owned plugin's capture grid and kept versions. A plugin that is
/// not a capture plugin has no grid (`null`) and is not editable.
pub fn plugin_grid_json(roots: &PluginRoots, plugin_id: &str) -> Result<String> {
    let (package, origin) = roots.owned(plugin_id)?;
    let manifest = read_disk_manifest(&package.root)?;
    let grid = capture_grid(&manifest)
        .map(|(parameters, captures)| read_grid(&package.root, parameters, captures));
    let json = serde_json::json!({
        "plugin_id": plugin_id,
        "origin": origin,
        "editable": grid.is_some(),
        "versions": version_numbers(&package.root),
        "grid": grid,
    });
    serde_json::to_string(&json).map_err(|e| anyhow!("{e}"))
}
