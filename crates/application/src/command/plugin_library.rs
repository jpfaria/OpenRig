//! Responsibility: names the plugin library commands.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::plugin_library::{CaptureBackend, EditorGrid};
use crate::tone3000::Tone3000BlockType;

/// Every change to a plugin the user owns. The plugins the app ships are
/// refused.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum PluginLibraryCommand {
    /// Rename the parameters of a capture plugin and the setting of each
    /// capture. Capture files never move; the save is kept as a new version.
    SavePluginParameters { plugin_id: String, grid: EditorGrid },

    /// Make a kept version the current one, saved as a new version.
    RestorePluginVersion { plugin_id: String, version: u32 },

    /// Read the capture names again and rebuild the parameters from them.
    /// A TONE3000 plugin whose tone changed is downloaded again first.
    RedoPluginParameters { plugin_id: String },

    /// Remove a plugin from disk and from the catalog.
    UninstallPlugin { plugin_id: String },

    /// Build a plugin in the plugins folder from capture files on disk.
    CreatePlugin {
        display_name: String,
        brand: Option<String>,
        block_type: Tone3000BlockType,
        backend: CaptureBackend,
        grid: EditorGrid,
    },
}
