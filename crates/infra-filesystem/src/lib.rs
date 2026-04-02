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
}

impl Default for AssetPaths {
    fn default() -> Self {
        Self {
            lv2_libs: default_lv2_libs(),
            lv2_data: default_lv2_data(),
            nam_captures: default_nam_captures(),
            ir_captures: default_ir_captures(),
        }
    }
}

fn default_lv2_libs() -> String {
    #[cfg(target_os = "macos")]
    { "libs/lv2/macos-universal".to_string() }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { "libs/lv2/linux-x86_64".to_string() }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { "libs/lv2/linux-aarch64".to_string() }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { "libs/lv2/windows-x64".to_string() }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    { "libs/lv2/windows-arm64".to_string() }
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

static ASSET_PATHS: OnceLock<AssetPaths> = OnceLock::new();

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
    ASSET_PATHS.get().expect("asset_paths not initialized — call init_asset_paths() during startup")
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioSettings {
    #[serde(default)]
    pub input_devices: Vec<GuiAudioDeviceSettings>,
    #[serde(default)]
    pub output_devices: Vec<GuiAudioDeviceSettings>,
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
            })
            .collect();
        Self {
            input_devices,
            output_devices,
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
mod tests {
    use super::{AppConfig, RecentProjectEntry};

    fn insert_recent_project(config: &mut AppConfig, entry: RecentProjectEntry) {
        config
            .recent_projects
            .retain(|current| current.project_path != entry.project_path);
        config.recent_projects.insert(0, entry);
    }

    #[test]
    fn recent_projects_move_existing_entry_to_top_without_duplication() {
        let mut config = AppConfig {
            recent_projects: vec![
                RecentProjectEntry {
                    project_path: "/b/project.yaml".into(),
                    project_name: "B".into(),
                    is_valid: true,
                    invalid_reason: None,
                },
                RecentProjectEntry {
                    project_path: "/a/project.yaml".into(),
                    project_name: "A".into(),
                    is_valid: true,
                    invalid_reason: None,
                },
            ],
            ..Default::default()
        };

        insert_recent_project(
            &mut config,
            RecentProjectEntry {
                project_path: "/a/project.yaml".into(),
                project_name: "A2".into(),
                is_valid: true,
                invalid_reason: None,
            },
        );

        assert_eq!(config.recent_projects.len(), 2);
        assert_eq!(config.recent_projects[0].project_path, "/a/project.yaml");
        assert_eq!(config.recent_projects[0].project_name, "A2");
        assert_eq!(config.recent_projects[1].project_path, "/b/project.yaml");
    }
}
