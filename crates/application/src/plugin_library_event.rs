//! Responsibility: names every observable change of the plugin library.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum PluginLibraryEvent {
    /// New parameters were saved as `version` and the catalog reloaded.
    Saved { plugin_id: String, version: u32 },

    /// A kept version was made current, saved as `version`.
    Restored { plugin_id: String, version: u32 },

    /// The parameters were rebuilt from the capture names, saved as
    /// `version`.
    Redone { plugin_id: String, version: u32 },

    /// Rebuilding the parameters failed; nothing changed.
    RedoFailed { plugin_id: String, message: String },

    /// The plugin left the disk and the catalog.
    Uninstalled { plugin_id: String },

    /// A new plugin is in the catalog.
    Created { plugin_id: String },

    /// Building a new plugin failed; nothing was left on disk.
    CreateFailed { message: String },
}
