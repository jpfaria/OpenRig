//! Responsibility: resolves where the app's own config files live.
//!
//! The system config sits at the root of the user folder
//! ([`crate::user_data_root`]): `~/.openrig` on macOS and Linux,
//! `%APPDATA%\OpenRig` on Windows.

use anyhow::Result;
use std::path::PathBuf;

use crate::{user_data_root, FilesystemStorage};

impl FilesystemStorage {
    pub fn gui_settings_path() -> Result<PathBuf> {
        Ok(user_data_root().join("gui-settings.yaml"))
    }

    pub fn app_config_path() -> Result<PathBuf> {
        Ok(user_data_root().join("config.yaml"))
    }
}
