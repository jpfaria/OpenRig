//! Responsibility: keeps the saved versions of a package's manifest.
//!
//! Every edit of a capture plugin's parameters is kept as
//! `versions/<n>.yaml` beside `manifest.yaml`, so an older version can be
//! restored and a block saved against it can find its capture again. The
//! files are not named `manifest.yaml`, so discovery never reads them as
//! packages.

use std::io;
use std::path::{Path, PathBuf};

use crate::manifest::PluginManifest;

pub const VERSIONS_DIR: &str = "versions";

/// The versions saved for the package at `package_root`, oldest first.
pub fn version_numbers(package_root: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(package_root.join(VERSIONS_DIR)) else {
        return Vec::new();
    };
    let mut versions: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "yaml" {
                return None;
            }
            path.file_stem()?.to_str()?.parse().ok()
        })
        .collect();
    versions.sort_unstable();
    versions
}

/// Version `version` of the package at `package_root`, when it is saved.
pub fn read_version(package_root: &Path, version: u32) -> Option<PluginManifest> {
    let text = std::fs::read_to_string(version_path(package_root, version)).ok()?;
    serde_yaml::from_str(&text).ok()
}

/// Saves `manifest` as version `version` of the package at `package_root`.
pub fn write_version(
    package_root: &Path,
    version: u32,
    manifest: &PluginManifest,
) -> io::Result<()> {
    std::fs::create_dir_all(package_root.join(VERSIONS_DIR))?;
    let yaml = serde_yaml::to_string(manifest).map_err(io::Error::other)?;
    std::fs::write(version_path(package_root, version), yaml)
}

fn version_path(package_root: &Path, version: u32) -> PathBuf {
    package_root
        .join(VERSIONS_DIR)
        .join(format!("{version}.yaml"))
}

#[cfg(test)]
#[path = "version_store_tests.rs"]
mod tests;
