//! Responsibility: manages the installed TONE3000 packages.

use std::path::{Path, PathBuf};

use plugin_loader::manifest::PluginManifest;
use plugin_loader::validate_manifest;

use super::install_error::InstallError;

const ID_PREFIX: &str = "tone3000_";
pub const MANIFEST_FILE: &str = "manifest.yaml";

/// A package on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledPlugin {
    pub plugin_id: String,
    pub dir: PathBuf,
    pub manifest: PluginManifest,
}

/// Valid TONE3000 packages under `root`, by id. Half-written or foreign
/// folders are skipped.
pub fn list_installed(root: &Path) -> Vec<InstalledPlugin> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<InstalledPlugin> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let plugin_id = entry.file_name().to_str()?.to_string();
            if !is_plugin_id(&plugin_id) {
                return None;
            }
            let dir = entry.path();
            let text = std::fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?;
            let manifest: PluginManifest = serde_yaml::from_str(&text).ok()?;
            (manifest.id == plugin_id && validate_manifest(&manifest).is_ok()).then_some(
                InstalledPlugin {
                    plugin_id,
                    dir,
                    manifest,
                },
            )
        })
        .collect();
    found.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    found
}

/// Deletes one package. Only a plain `tone3000_*` folder name directly
/// under `root` is accepted.
pub fn remove_installed(root: &Path, plugin_id: &str) -> Result<(), InstallError> {
    if !is_plugin_id(plugin_id) {
        return Err(InstallError::InvalidPluginId(plugin_id.to_string()));
    }
    let dir = root.join(plugin_id);
    if !dir.is_dir() {
        return Err(InstallError::NotInstalled(plugin_id.to_string()));
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

pub fn is_plugin_id(id: &str) -> bool {
    id.strip_prefix(ID_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}
