use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

pub mod midi_device;
pub mod midi_migrate;
pub mod midi_profile;
pub use midi_device::{MidiDeviceSelection, MidiPortKey};

#[cfg(test)]
#[path = "midi_profile_tests.rs"]
mod midi_profile_tests;

#[cfg(test)]
#[path = "midi_migrate_tests.rs"]
mod midi_migrate_tests;

/// Central configuration for asset directories used by the engine and GUI.
///
/// Each field holds a path (absolute or relative to the executable) where
/// the corresponding asset category lives. When the app starts it loads
/// these values from `config.yaml` (falling back to sensible defaults) and
/// stores them in a global `OnceLock` so every crate can access them
/// without passing config around.
///
/// Plugin assets — NAM/IR captures, LV2 binaries and metadata — moved to
/// the OpenRig-plugins repo in issue #287 and are resolved via
/// [`plugin_loader::config::plugins_root_from_config`], NOT through this
/// struct. Only UI-side asset categories live here now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPaths {
    /// Root directory for block thumbnails (PNG images).
    #[serde(default = "default_thumbnails")]
    pub thumbnails: String,
    /// Root directory for block screenshots (PNG images for info panel).
    #[serde(default = "default_screenshots")]
    pub screenshots: String,
    /// Root directory for plugin metadata YAML files (per-language).
    #[serde(default = "default_metadata")]
    pub metadata: String,
    /// #513: user-chosen directory holding project preset libraries. `None`
    /// keeps the historical OS default (the launcher resolves it). When set,
    /// this override wins for preset discovery / save dialogs.
    #[serde(default)]
    pub presets_path: Option<PathBuf>,
    /// #513: user-chosen directory holding plugin packs (NAM/IR/LV2). `None`
    /// keeps the historical OS default resolved by
    /// `plugin_loader::config::plugins_root_from_config`. When set, this
    /// override wins for plugin scanning.
    #[serde(default)]
    pub plugins_path: Option<PathBuf>,
    /// #582: user-chosen directory where tone analyzers and other tools
    /// write evaluation artifacts (spectrograms, fingerprints, comparison
    /// reports). `None` keeps the OS default resolved by
    /// [`default_evaluations_path`]. Machine-local concern per ADR 0003 —
    /// lives in `config.yaml`, not the project YAML.
    #[serde(default)]
    pub evaluations_path: Option<PathBuf>,
}

impl Default for AssetPaths {
    fn default() -> Self {
        Self {
            thumbnails: default_thumbnails(),
            screenshots: default_screenshots(),
            metadata: default_metadata(),
            presets_path: None,
            plugins_path: None,
            evaluations_path: None,
        }
    }
}

fn default_thumbnails() -> String {
    "assets/blocks/thumbnails".to_string()
}

fn default_screenshots() -> String {
    "assets/blocks/screenshots".to_string()
}

fn default_metadata() -> String {
    "assets/blocks/metadata".to_string()
}

static ASSET_PATHS: OnceLock<AssetPaths> = OnceLock::new();

/// Detect the application data root for the current installation layout.
///
/// Returns the directory that contains `libs/`, `data/`, and `assets/`:
///
/// - macOS `.app` bundle: `<bundle>/Contents/Resources/`
/// - Linux deb/rpm: `/usr/share/openrig/`
/// - Windows MSI: directory alongside the executable
/// - Development fallback: current working directory
pub fn detect_data_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        #[cfg(target_os = "macos")]
        if let Some(resources) = exe
            .parent() // .app/Contents/MacOS/
            .and_then(|p| p.parent()) // .app/Contents/
            .map(|p| p.join("Resources"))
        {
            if resources.exists() {
                return resources;
            }
        }

        #[cfg(target_os = "linux")]
        if let Some(exe_dir) = exe.parent() {
            if let Some(prefix) = exe_dir.parent() {
                let share = prefix.join("share/openrig");
                if share.exists() {
                    return share;
                }
            }
        }

        #[cfg(target_os = "windows")]
        if let Some(exe_dir) = exe.parent() {
            if exe_dir.join("libs").exists() {
                return exe_dir.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Resolve relative asset paths against the detected data root.
///
/// Absolute paths in `paths` are left unchanged. Relative paths are joined
/// with `detect_data_root()` so the app finds its assets regardless of the
/// current working directory.
pub fn resolve_asset_paths(paths: AssetPaths) -> AssetPaths {
    let root = detect_data_root();
    fn resolve(root: &std::path::Path, s: String) -> String {
        let p = std::path::Path::new(&s);
        if p.is_absolute() {
            s
        } else {
            root.join(p).to_string_lossy().into_owned()
        }
    }
    AssetPaths {
        thumbnails: resolve(&root, paths.thumbnails),
        screenshots: resolve(&root, paths.screenshots),
        metadata: resolve(&root, paths.metadata),
        // #513: user overrides are stored absolute (file picker resolves them).
        // No data-root rebase — `None` means "use the OS default" and is the
        // signal the resolvers look for. Same applies to #582's
        // `evaluations_path`.
        presets_path: paths.presets_path,
        plugins_path: paths.plugins_path,
        evaluations_path: paths.evaluations_path,
    }
}

/// #582: OS default for the evaluations directory (tone analyzer outputs,
/// fingerprint snapshots, A/B comparison reports). Per CLAUDE.md
/// cross-platform rule:
///
/// - macOS: `~/Library/Application Support/OpenRig/evaluations/`
/// - Windows: `%APPDATA%\OpenRig\evaluations\`
/// - Linux: `~/.local/share/openrig/evaluations/`
///
/// Used when [`AssetPaths::evaluations_path`] is `None`. Returns the path
/// without creating it — callers materialize the directory only when they
/// actually write into it.
pub fn default_evaluations_path() -> PathBuf {
    user_data_root().join("evaluations")
}

/// #582: OS-specific user data root for OpenRig
/// (`~/Library/Application Support/OpenRig` on macOS,
/// `%APPDATA%\OpenRig` on Windows,
/// `~/.local/share/openrig` on Linux). Mirrors the same convention
/// `FilesystemStorage::app_config_path` uses, kept as a shared helper so
/// every `default_*_path` derived from it stays consistent.
pub fn user_data_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join("Library/Application Support/OpenRig")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        appdata.join("OpenRig")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".local/share/openrig")
    }
}

/// Store the resolved asset paths for the lifetime of the process.
///
/// Must be called once during app startup (after loading config).  Subsequent
/// calls are silently ignored so that tests that initialise multiple times do
/// not panic.
pub fn init_asset_paths(paths: AssetPaths) {
    ASSET_PATHS.set(paths).ok();
}

/// Retrieve the global asset paths.
///
/// # Panics
/// Panics if `init_asset_paths` has not been called yet.
pub fn asset_paths() -> &'static AssetPaths {
    ASSET_PATHS
        .get()
        .expect("asset_paths not initialized — call init_asset_paths() during startup")
}

pub struct FilesystemStorage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecentProjectEntry {
    pub project_path: String,
    pub project_name: String,
    #[serde(default = "default_true")]
    pub is_valid: bool,
    #[serde(default)]
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioDeviceSettings {
    pub device_id: String,
    pub name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    pub buffer_size_frames: u32,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u32,
    // Linux JACK tuning — only present on Linux builds. cpal backends on
    // macOS (CoreAudio) and Windows (WASAPI/ASIO) don't honour realtime
    // priority or ALSA nperiods, so the fields don't exist there and the
    // YAML stays clean.
    #[cfg(target_os = "linux")]
    #[serde(default = "default_realtime")]
    pub realtime: bool,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_rt_priority")]
    pub rt_priority: u8,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_nperiods")]
    pub nperiods: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiSystemSettings {
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    // Renamed from GuiAudioSettings (#513) to reflect that it holds every
    // per-machine GUI preference, not just audio.
    // None / "auto" follows the OS locale; "pt-BR" / "en-US" override it.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub midi_devices: Vec<MidiDeviceSelection>,
}

impl GuiSystemSettings {
    pub fn is_complete(&self) -> bool {
        !self.input_devices.is_empty() && !self.output_devices.is_empty()
    }
}

fn default_sample_rate() -> u32 {
    48_000
}

fn default_buffer_size_frames() -> u32 {
    256
}

fn default_bit_depth() -> u32 {
    32
}

#[cfg(target_os = "linux")]
fn default_realtime() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn default_rt_priority() -> u8 {
    70
}

#[cfg(target_os = "linux")]
fn default_nperiods() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(crate) struct LegacyGuiAudioSettings {
    #[serde(default)]
    input_device_names: Vec<String>,
    #[serde(default)]
    output_device_names: Vec<String>,
    #[serde(default = "default_sample_rate")]
    sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    buffer_size_frames: u32,
}

impl From<LegacyGuiAudioSettings> for GuiSystemSettings {
    fn from(value: LegacyGuiAudioSettings) -> Self {
        let input_devices = value
            .input_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
                device_id: String::new(),
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
                bit_depth: default_bit_depth(),
                #[cfg(target_os = "linux")]
                realtime: default_realtime(),
                #[cfg(target_os = "linux")]
                rt_priority: default_rt_priority(),
                #[cfg(target_os = "linux")]
                nperiods: default_nperiods(),
            })
            .collect();
        let output_devices = value
            .output_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
                device_id: String::new(),
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
                bit_depth: default_bit_depth(),
                #[cfg(target_os = "linux")]
                realtime: default_realtime(),
                #[cfg(target_os = "linux")]
                rt_priority: default_rt_priority(),
                #[cfg(target_os = "linux")]
                nperiods: default_nperiods(),
            })
            .collect();
        Self {
            input_devices,
            output_devices,
            language: None,
            midi_devices: vec![],
        }
    }
}

impl FilesystemStorage {
    pub fn gui_settings_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("gui-settings.yaml"))
    }

    pub fn app_config_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("config.yaml"))
    }

    /// Per-OS path of the **legacy** single-file MIDI mapping (`adapter-midi`,
    /// issue #22). macOS `~/Library/Application Support/OpenRig/midi-map.yaml`,
    /// Windows `%APPDATA%\OpenRig\midi-map.yaml`, Linux
    /// `~/.config/OpenRig/midi-map.yaml`. Never hardcoded — resolved like every
    /// other OpenRig config file. After #499 this file is migrated on first
    /// load into the system [`midi_profile_path`] + system [`midi_bindings_path`]
    /// and then deleted; this getter survives only for the migration path.
    pub fn midi_map_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("midi-map.yaml"))
    }

    /// Per-OS path of the **MIDI device profile** (ADR 0003 / #499): which
    /// controller to listen to. System layer; never overridden by the project.
    /// macOS `~/Library/Application Support/OpenRig/midi-profile.yaml`,
    /// Windows `%APPDATA%\OpenRig\midi-profile.yaml`, Linux
    /// `~/.config/OpenRig/midi-profile.yaml`.
    pub fn midi_profile_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("midi-profile.yaml"))
    }

    /// Per-OS path of the **system-wide MIDI bindings fallback** (ADR 0003 /
    /// #499). Used at resolve time when a project carries no `midi:` field;
    /// the shipped default ships as `examples/midi-map.default.yaml` and the
    /// system fallback overrides it when present. Same per-OS layout as the
    /// other config files.
    pub fn midi_bindings_path() -> Result<PathBuf> {
        let base_dir = dirs::config_dir()
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("midi-bindings.yaml"))
    }
}

#[path = "app_config_io.rs"]
mod app_config_io;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
