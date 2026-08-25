//! Responsibility: resolves the per-OS location of the app's own config files.
//!
//! Split out of `lib.rs` (#873). Never hardcoded: macOS
//! `~/Library/Application Support/OpenRig`, Windows `%APPDATA%\OpenRig`,
//! Linux `~/.config/OpenRig`.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::FilesystemStorage;

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
}
