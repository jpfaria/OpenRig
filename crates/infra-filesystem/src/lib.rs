use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub struct FilesystemStorage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioDeviceSettings {
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
                name,
                sample_rate: value.sample_rate,
                buffer_size_frames: value.buffer_size_frames,
            })
            .collect();
        let output_devices = value
            .output_device_names
            .into_iter()
            .map(|name| GuiAudioDeviceSettings {
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
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from).map(|home| home.join(".config")))
            .context("failed to resolve user config directory")?;
        Ok(base_dir.join("OpenRig").join("gui-settings.yaml"))
    }

    pub fn load_gui_audio_settings() -> Result<Option<GuiAudioSettings>> {
        let path = Self::gui_settings_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read gui settings from {:?}", path))?;
        let settings = match serde_yaml::from_str::<GuiAudioSettings>(&raw) {
            Ok(settings) => settings,
            Err(_) => {
                let legacy = serde_yaml::from_str::<LegacyGuiAudioSettings>(&raw)
                    .with_context(|| format!("failed to parse gui settings from {:?}", path))?;
                legacy.into()
            }
        };
        Ok(Some(settings))
    }

    pub fn save_gui_audio_settings(settings: &GuiAudioSettings) -> Result<()> {
        let path = Self::gui_settings_path()?;
        let parent = path
            .parent()
            .context("gui settings path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create gui settings directory {:?}", parent))?;
        let raw = serde_yaml::to_string(settings)?;
        fs::write(&path, raw)
            .with_context(|| format!("failed to write gui settings to {:?}", path))?;
        Ok(())
    }
}
