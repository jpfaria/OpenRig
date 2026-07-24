//! System-level settings commands: audio device selection, UI language, the
//! configurable directories, and the MCP server master switch.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Every state change scoped to a setting rather than to project content.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SettingsCommand {
    /// Persist the current audio device selection into the project and
    /// resync the audio runtime.
    ///
    /// The adapter collects the selected device rows and resolves them to
    /// `DeviceSettings` before dispatching. The dispatcher replaces the
    /// project's `device_settings` with the provided list.
    SaveAudioSettings {
        /// Devices selected as inputs. Persisted into `config.input_devices`.
        input_devices: Vec<project::device::DeviceSettings>,
        /// Devices selected as outputs. Persisted into `config.output_devices`.
        ///
        /// Kept separate from `input_devices` because the same physical
        /// interface enumerates with a different `device_id` per direction
        /// (CoreAudio/WASAPI); collapsing both into one flat list corrupts the
        /// saved selection and breaks re-match on reopen (#581 follow-up).
        output_devices: Vec<project::device::DeviceSettings>,
    },

    /// #436 F: set the UI language preference. Was GUI-only
    /// (`FilesystemStorage::save_gui_language` + live i18n swap in a
    /// wiring closure). Now a Command so MIDI/MCP can request it too.
    /// Follows the `SaveProject` precedent: the adapter performs the
    /// persistence + live swap; the dispatcher records the intent and
    /// signals it via `Event::LanguageChanged`. `None` = system default.
    SetLanguage { language: Option<String> },

    /// #513: persist the user's preferred directory for project preset
    /// libraries. `None` resets to the OS default (the existing resolver
    /// wins again). System-level setting per ADR 0003 — the adapter
    /// writes it into `config.yaml` on `Event::PathsSaved`.
    SetPresetsPath { path: Option<PathBuf> },

    /// #513: persist the user's preferred directory for plugin scanning
    /// (NAM/IR/LV2 packs). `None` resets to the OS default. System-level
    /// setting per ADR 0003 — the adapter writes it into `config.yaml`
    /// on `Event::PathsSaved`.
    SetPluginsPath { path: Option<PathBuf> },

    /// #582: persist the user's preferred directory for evaluation
    /// artifacts (tone-analyzer outputs, fingerprints, comparison
    /// reports). `None` resets to the OS default
    /// ([`infra_filesystem::default_evaluations_path`]). System-level
    /// setting per ADR 0003 — the adapter writes it into `config.yaml`
    /// on `Event::PathsSaved`.
    SetEvaluationsPath { path: Option<PathBuf> },

    /// #712: master switch for the MCP server, persisted into
    /// `config.yaml` (`mcp_enabled`). Same per-machine, restart-to-apply
    /// contract as [`Command::SetMidiEnabled`].
    SetMcpEnabled { enabled: bool },
}
