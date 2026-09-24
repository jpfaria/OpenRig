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
        Ok(config_dir()?.join("OpenRig").join("gui-settings.yaml"))
    }

    pub fn app_config_path() -> Result<PathBuf> {
        Ok(config_dir()?.join("OpenRig").join("config.yaml"))
    }
}

/// The per-user config directory. On Windows `%APPDATA%` comes first, the
/// same lookup `user_data_root` uses: the Known Folder API ignores the
/// environment, so nothing (a test included) could point the app's config
/// anywhere else, and HOME-swapping tests wrote the real config (#978).
fn config_dir() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let base =
        crate::config_base::windows_config_base(std::env::var_os("APPDATA"), dirs::config_dir());
    #[cfg(not(target_os = "windows"))]
    let base = dirs::config_dir().or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".config"))
    });
    base.context("failed to resolve user config directory")
}
