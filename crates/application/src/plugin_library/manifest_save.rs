//! Responsibility: makes a new manifest current as the next kept version.
//!
//! The first save also keeps the manifest it replaces as version 1. The
//! manifest is written through a temp file, then the catalog drops the old
//! entry and loads the package again.

use std::path::Path;

use anyhow::{Context, Result};
use plugin_loader::manifest::PluginManifest;
use plugin_loader::version_store::{version_numbers, write_version};

use crate::tone3000::installed::MANIFEST_FILE;

/// Saves `next` over `current` in the package at `package_root` and
/// returns the version it was kept as.
pub fn save_manifest_version(
    package_root: &Path,
    current: &PluginManifest,
    next: &PluginManifest,
) -> Result<u32> {
    plugin_loader::validate_manifest(next).map_err(|e| anyhow::anyhow!("{e}"))?;
    let versions = version_numbers(package_root);
    if versions.is_empty() {
        write_version(package_root, 1, current).context("keeping the current version")?;
    }
    let version = versions.last().copied().unwrap_or(1) + 1;
    write_version(package_root, version, next).context("keeping the new version")?;
    write_manifest(package_root, next)?;
    reload(package_root, &next.id)?;
    Ok(version)
}

/// Writes `manifest` as the package's manifest, never leaving half a file.
pub fn write_manifest(package_root: &Path, manifest: &PluginManifest) -> Result<()> {
    let yaml = serde_yaml::to_string(manifest)?;
    let tmp = package_root.join(format!(".{MANIFEST_FILE}.tmp"));
    std::fs::write(&tmp, yaml).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, package_root.join(MANIFEST_FILE))?;
    Ok(())
}

/// Drops `plugin_id` from the catalog and loads it again from disk.
pub fn reload(package_root: &Path, plugin_id: &str) -> Result<()> {
    let _ = plugin_loader::registry::unload(plugin_id);
    let parent = package_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    plugin_loader::registry::load_one(plugin_id, &[parent])
        .map_err(|e| anyhow::anyhow!("saved, but the catalog did not load it: {e}"))
}
