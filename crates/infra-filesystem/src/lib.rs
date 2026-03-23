use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
