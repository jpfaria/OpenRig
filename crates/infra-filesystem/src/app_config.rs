//! Responsibility: describes the shape of the per-machine `config.yaml`.
//!
//! Split out of `lib.rs` (#873). Reading and writing the file is
//! [`crate::app_config_io`]; this file only says what is in it.

use serde::{Deserialize, Serialize};

use crate::appearance::Appearance;
use crate::asset_paths::AssetPaths;
use crate::crash_reporting_config::CrashReportingConfig;
use crate::drums_config::DrumsConfig;
use crate::gui_settings::GuiAudioDeviceSettings;
use crate::io_bindings::IoBinding;
use crate::metronome_config::MetronomeConfig;
use crate::midi_device::MidiDeviceSelection;
use crate::mixer_config::MixerStripConfig;
use crate::player_config::PlayerConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecentProjectEntry {
    pub project_path: String,
    pub project_name: String,
    #[serde(default = "default_true")]
    pub is_valid: bool,
    #[serde(default)]
    pub invalid_reason: Option<String>,
}

// `Eq` is not derivable: `MetronomeConfig` carries floats (#14).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub recent_projects: Vec<RecentProjectEntry>,
    #[serde(default)]
    pub paths: AssetPaths,
    /// Per-machine audio input devices. Migrated from the historical
    /// `gui-settings.yaml` (deleted automatically on first load).
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    /// Per-machine audio output devices.
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    /// Language override (`pt-BR`, `en-US`, etc.). `None` follows OS
    /// locale.
    #[serde(default)]
    pub language: Option<String>,
    /// #398: light/dark scheme of the interface. `system` (the default)
    /// follows the operating system.
    #[serde(default)]
    pub appearance: Appearance,
    /// Per-machine MIDI device selection (#513). Empty list = none seen
    /// yet; the GUI seeds rows from `adapter_midi::list_input_ports()`.
    #[serde(default)]
    pub midi_devices: Vec<MidiDeviceSelection>,
    /// Master switch for the MIDI/BLE-MIDI adapter (#712). Per-machine, so
    /// it lives here, not in the project (ADR 0003). Default `false`: a
    /// packaged build stays quiet until the user opts in (Settings toggle
    /// or `--midi`, which overrides this for the run). Distinct from the
    /// per-port `midi_devices[].enabled` selection — this gates the whole
    /// subsystem.
    #[serde(default)]
    pub midi_enabled: bool,
    /// Master switch for the MCP server (#712). Per-machine; default
    /// `false`. `--mcp` / `--mcp=ADDR` overrides it for the run.
    #[serde(default)]
    pub mcp_enabled: bool,
    /// Per-machine I/O binding registry (#716). Maps stable binding ids to
    /// the physical device endpoints they represent, so projects can reference
    /// endpoints by name and remain portable across machines.
    ///
    /// `#[serde(default)]` ensures legacy `config.yaml` files that predate
    /// this field still deserialize correctly (field absent → empty `Vec`).
    #[serde(default)]
    pub io_bindings: Vec<IoBinding>,
    /// Per-machine metronome settings (#14, ADR 0003). `enabled` is absent
    /// on purpose — see [`MetronomeConfig`].
    #[serde(default)]
    pub metronome: MetronomeConfig,
    /// Per-machine global mixer (#1007, ADR 0003): only the strips moved
    /// away from unity or muted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mixer: Vec<MixerStripConfig>,
    /// Per-machine backing-track player settings (ADR 0003).
    #[serde(default)]
    pub player: PlayerConfig,
    /// Per-machine drum machine settings (ADR 0003). The transport is absent
    /// on purpose — see [`DrumsConfig`].
    #[serde(default)]
    pub drums: DrumsConfig,
    /// Per-machine crash reporter choice (#1070, ADR 0003).
    #[serde(default)]
    pub crash_reporting: CrashReportingConfig,
}

fn default_true() -> bool {
    true
}
