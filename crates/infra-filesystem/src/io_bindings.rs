//! Responsibility: persists the machine's registry of endpoint bindings.
//!
//! The types themselves come from `domain::io_binding` — one definition
//! shared with `project`, which references bindings by id.

use anyhow::Result;

pub use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

use crate::FilesystemStorage;

impl FilesystemStorage {
    /// #716: update only the I/O binding registry in `config.yaml`, preserving
    /// every other config field. Consumers that need to replace the whole
    /// registry at once call this rather than loading and saving AppConfig
    /// directly.
    pub fn save_io_bindings(bindings: Vec<IoBinding>) -> Result<()> {
        let mut config = Self::load_app_config().unwrap_or_default();
        config.io_bindings = bindings;
        Self::save_app_config(&config)
    }
}
