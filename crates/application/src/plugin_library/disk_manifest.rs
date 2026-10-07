//! Responsibility: reads a package's manifest as it is on disk.
//!
//! The catalog keeps a manifest trimmed to the values its captures back;
//! the editor and its saves work on the file itself.

use std::path::Path;

use anyhow::{Context, Result};
use plugin_loader::manifest::PluginManifest;

use crate::tone3000::installed::MANIFEST_FILE;

pub fn read_disk_manifest(package_root: &Path) -> Result<PluginManifest> {
    let path = package_root.join(MANIFEST_FILE);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}
