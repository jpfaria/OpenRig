//! Plugin catalog commands (#561): re-scan the plugin roots, or load/unload a
//! single disk plugin by manifest id without a session break.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every state change scoped to the in-memory plugin catalog.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum PluginCommand {
    /// #561: re-scan the plugin packages directories without restarting
    /// the process. Same path resolution as boot
    /// (`detect_data_root().join("plugins")` + `plugins_root_from_config`),
    /// natives are preserved. The dispatcher emits
    /// `Event::PluginCatalogReloaded { native_count, disk_count,
    /// total_count }` so adapters can surface the new totals to the
    /// user (GUI toast, MCP tool response). Closes the gap between
    /// "import a new NAM" and "build a preset that uses it" without a
    /// session break.
    ReloadPluginCatalog,

    /// #561 (expanded scope): bring a single disk plugin into the
    /// in-memory catalog by manifest id. Re-scans the known plugin
    /// roots and adds the one whose id matches. Errors when no disk
    /// package with that id is discoverable; no-op when the plugin
    /// is already loaded. Emits `Event::PluginLoaded { id }`.
    LoadPlugin { id: String },

    /// #561 (expanded scope): remove a single disk plugin from the
    /// in-memory catalog by manifest id. Refuses natives — they are
    /// compiled-in and cannot be dropped without restarting the
    /// process. Emits `Event::PluginUnloaded { id }`.
    UnloadPlugin { id: String },
}
