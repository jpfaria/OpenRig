use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub struct FilesystemStorage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuiAudioSettings {
    pub input_device_names: Vec<String>,
    pub output_device_names: Vec<String>,
}

impl GuiAudioSettings {
    pub fn is_complete(&self) -> bool {
        !self.input_device_names.is_empty() && !self.output_device_names.is_empty()
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
        let settings = serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse gui settings from {:?}", path))?;
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
