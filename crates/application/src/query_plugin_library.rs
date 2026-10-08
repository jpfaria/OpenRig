//! Responsibility: answers what the plugin library holds for every transport.

use anyhow::{anyhow, Result};
use plugin_loader::version_store::version_numbers;
use project::block::grid_version_follow::capture_grid;
use serde::Serialize;

use crate::plugin_library::disk_manifest::read_disk_manifest;
use crate::plugin_library::entries::library_entries;
use crate::plugin_library::grid_read::read_grid;
use crate::plugin_library::{EditorGrid, PluginOrigin, PluginRoots};

/// One owned plugin as the editor opens it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginGridView {
    pub plugin_id: String,
    pub origin: PluginOrigin,
    /// A capture plugin, whose parameters the editor can change.
    pub editable: bool,
    /// The kept versions of its parameters, oldest first.
    pub versions: Vec<u32>,
    /// `None` for a plugin that is not a capture plugin.
    pub grid: Option<EditorGrid>,
}

/// Every plugin the user owns, as `{"plugins": [...]}`.
pub fn plugin_library_json(roots: &PluginRoots) -> String {
    serde_json::json!({ "plugins": library_entries(roots) }).to_string()
}

/// One owned plugin's capture grid and kept versions.
pub fn plugin_grid(roots: &PluginRoots, plugin_id: &str) -> Result<PluginGridView> {
    let (package, origin) = roots.owned(plugin_id)?;
    let manifest = read_disk_manifest(&package.root)?;
    let grid = capture_grid(&manifest)
        .map(|(parameters, captures)| read_grid(&package.root, parameters, captures));
    Ok(PluginGridView {
        plugin_id: plugin_id.to_string(),
        origin,
        editable: grid.is_some(),
        versions: version_numbers(&package.root),
        grid,
    })
}

/// [`plugin_grid`] as JSON. A plugin that is not a capture plugin has no
/// grid (`null`) and is not editable. A plugin the catalog does not hold
/// answers the same shape, empty: no origin, no versions, no grid.
pub fn plugin_grid_json(roots: &PluginRoots, plugin_id: &str) -> Result<String> {
    if plugin_loader::registry::find(plugin_id).is_none() {
        return Ok(serde_json::json!({
            "plugin_id": plugin_id,
            "origin": null,
            "editable": false,
            "versions": [],
            "grid": null,
        })
        .to_string());
    }
    serde_json::to_string(&plugin_grid(roots, plugin_id)?).map_err(|e| anyhow!("{e}"))
}
