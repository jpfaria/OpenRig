//! Responsibility: persists the machine's registry of endpoint bindings.
//!
//! The types themselves come from `domain::io_binding` — one definition
//! shared with `project`, which references bindings by id.

use anyhow::Result;
use std::path::Path;

pub use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

use crate::FilesystemStorage;

impl FilesystemStorage {
    /// #716: update only the I/O binding registry in `config.yaml`, preserving
    /// every other config field. Consumers that need to replace the whole
    /// registry at once call this rather than loading and saving AppConfig
    /// directly.
    pub fn save_io_bindings(bindings: Vec<IoBinding>) -> Result<()> {
        let path = Self::app_config_path()?;
        Self::save_io_bindings_at(&path, bindings)
    }

    /// [`Self::save_io_bindings`] against an explicit config file. The
    /// registry replacement is the same either way; only the location differs,
    /// so a test can exercise the preserve-everything-else contract without
    /// writing to the machine's real `config.yaml`.
    pub fn save_io_bindings_at(config_path: &Path, bindings: Vec<IoBinding>) -> Result<()> {
        let mut config = Self::load_app_config_at(config_path).unwrap_or_default();
        config.io_bindings = bindings;
        Self::save_app_config_at(config_path, &config)
    }
}

#[cfg(test)]
#[path = "io_bindings_tests.rs"]
mod tests;
