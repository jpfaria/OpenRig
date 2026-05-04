use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Central configuration for all asset directories.
///
/// Each field holds a path (absolute or relative to the executable) where the
/// corresponding asset category lives.  When the app starts it loads these
/// values from `config.yaml` (falling back to sensible per-platform defaults)
/// and stores them in a global `OnceLock` so every crate can access them
/// without passing config around.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPaths {
    /// Directory containing prebuilt LV2 shared libraries (.dylib/.so/.dll).
    #[serde(default = "default_lv2_libs")]
    pub lv2_libs: String,
    /// Directory containing LV2 plugin data (TTL metadata, presets).
    #[serde(default = "default_lv2_data")]
    pub lv2_data: String,
    /// Root directory for NAM capture files (.nam).
    #[serde(default = "default_nam_captures")]
    pub nam_captures: String,
    /// Root directory for IR capture files (.wav).
    #[serde(default = "default_ir_captures")]
    pub ir_captures: String,
    /// Root directory for block thumbnails (PNG images).
    #[serde(default = "default_thumbnails")]
    pub thumbnails: String,
    /// Root directory for block screenshots (PNG images for info panel).
    #[serde(default = "default_screenshots")]
    pub screenshots: String,
    /// Root directory for plugin metadata YAML files (per-language).
    #[serde(default = "default_metadata")]
    pub metadata: String,
}

impl Default for AssetPaths {
    fn default() -> Self {
        Self {
            lv2_libs: default_lv2_libs(),
            lv2_data: default_lv2_data(),
            nam_captures: default_nam_captures(),
            ir_captures: default_ir_captures(),
            thumbnails: default_thumbnails(),
            screenshots: default_screenshots(),
            metadata: default_metadata(),
        }
    }
}

fn default_lv2_libs() -> String {
    #[cfg(target_os = "macos")]
    {
        "libs/lv2/macos-universal".to_string()
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "libs/lv2/linux-x86_64".to_string()
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "libs/lv2/linux-aarch64".to_string()
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "libs/lv2/windows-x64".to_string()
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "libs/lv2/windows-arm64".to_string()
    }
}

fn default_lv2_data() -> String {
    "data/lv2".to_string()
}

fn default_nam_captures() -> String {
    "captures/nam".to_string()
}

fn default_ir_captures() -> String {
    "captures/ir".to_string()
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
        lv2_libs: resolve(&root, paths.lv2_libs),
        lv2_data: resolve(&root, paths.lv2_data),
        nam_captures: resolve(&root, paths.nam_captures),
        ir_captures: resolve(&root, paths.ir_captures),
        thumbnails: resolve(&root, paths.thumbnails),
        screenshots: resolve(&root, paths.screenshots),
        metadata: resolve(&root, paths.metadata),
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
pub struct GuiAudioSettings {
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
    // The struct name is historical (originally audio-only); the file lives
    // at gui-settings.yaml and now hosts every per-machine GUI preference.
    // None / "auto" follows the OS locale; "pt-BR" / "en-US" override it.
    #[serde(default)]
    pub language: Option<String>,
}

impl GuiAudioSettings {
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
struct LegacyGuiAudioSettings {
    #[serde(default)]
    input_device_names: Vec<String>,
    #[serde(default)]
    output_device_names: Vec<String>,
    #[serde(default = "default_sample_rate")]
    sample_rate: u32,
    #[serde(default = "default_buffer_size_frames")]
    buffer_size_frames: u32,
}

impl From<LegacyGuiAudioSettings> for GuiAudioSettings {
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

    pub fn load_gui_audio_settings() -> Result<Option<GuiAudioSettings>> {
        let path = Self::gui_settings_path()?;
        log::info!("loading gui audio settings from {:?}", path);
        if !path.exists() {
            log::debug!("gui audio settings file not found, returning None");
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read gui settings from {:?}", path))?;
        let settings = match serde_yaml::from_str::<GuiAudioSettings>(&raw) {
            Ok(settings) => settings,
            Err(_) => {
                log::warn!("failed to parse gui settings as current format, trying legacy format");
                let legacy = serde_yaml::from_str::<LegacyGuiAudioSettings>(&raw)
                    .with_context(|| format!("failed to parse gui settings from {:?}", path))?;
                legacy.into()
            }
        };
        Ok(Some(settings))
    }

    pub fn save_gui_audio_settings(settings: &GuiAudioSettings) -> Result<()> {
        let path = Self::gui_settings_path()?;
        log::info!("saving gui audio settings to {:?}", path);
        let parent = path
            .parent()
            .context("gui settings path has no parent directory")?;
        log::debug!("ensuring directory exists: {:?}", parent);
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create gui settings directory {:?}", parent))?;
        let raw = serde_yaml::to_string(settings)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write gui settings to {:?}", path))?;
        Ok(())
    }

    /// Update only the `language` field of the persisted gui-settings.yaml,
    /// preserving every other field. Used by the language selector so picking
    /// a new locale doesn't clobber audio device selection.
    pub fn save_gui_language(language: Option<String>) -> Result<()> {
        let mut current = Self::load_gui_audio_settings()?.unwrap_or_default();
        current.language = language;
        Self::save_gui_audio_settings(&current)
    }

    pub fn load_app_config() -> Result<AppConfig> {
        let path = Self::app_config_path()?;
        log::info!("loading app config from {:?}", path);
        if !path.exists() {
            log::debug!("app config file not found, using defaults");
            return Ok(AppConfig::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read app config from {:?}", path))?;
        serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse app config from {:?}", path))
    }

    pub fn save_app_config(config: &AppConfig) -> Result<()> {
        let path = Self::app_config_path()?;
        log::info!("saving app config to {:?}", path);
        let parent = path
            .parent()
            .context("app config path has no parent directory")?;
        log::debug!("ensuring directory exists: {:?}", parent);
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create app config directory {:?}", parent))?;
        let raw = serde_yaml::to_string(config)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write app config to {:?}", path))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
