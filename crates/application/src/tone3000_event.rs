//! Responsibility: names every observable change of the TONE3000 browser.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tone3000::install::InstallProgress;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Tone3000Event {
    /// The Secret Key was set (`true`) or cleared (`false`). The key itself
    /// never travels in an event.
    KeyChanged { configured: bool },

    /// A search answered; the results are in the browser state.
    SearchFinished { total: u32 },

    /// A search failed (bad key, rate limit, network).
    SearchFailed { message: String },

    /// An install moved on to its next step.
    InstallProgress {
        tone_id: u64,
        progress: InstallProgress,
    },

    /// A tone was installed and its plugin is in the catalog.
    Installed { tone_id: u64, plugin_id: String },

    /// An install stopped; nothing of it was left on disk.
    InstallFailed { tone_id: u64, message: String },

    /// A TONE3000 plugin was removed from disk and from the catalog.
    Uninstalled { plugin_id: String },
}
