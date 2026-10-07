//! Responsibility: manages the TONE3000 packages in the plugins folder.
//!
//! Packages live where the rest of the user's plugins do:
//! `<plugins>/nam/<id>` and `<plugins>/ir/<id>` (#879).

use std::path::{Path, PathBuf};

use plugin_loader::manifest::PluginManifest;
use plugin_loader::validate_manifest;

use super::install_error::InstallError;
use super::source_stamp::read_updated_at;

const ID_PREFIX: &str = "tone3000_";
pub const MANIFEST_FILE: &str = "manifest.yaml";

/// A package on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledPlugin {
    pub plugin_id: String,
    pub dir: PathBuf,
    pub manifest: PluginManifest,
    /// The TONE3000 tone version it holds; `None` when unknown.
    pub updated_at: Option<String>,
}

/// The plugins-folder subfolder for each kind of package.
pub const NAM_FOLDER: &str = "nam";
pub const IR_FOLDER: &str = "ir";

/// Valid browser packages (`tone3000_*`) under `root/nam` and `root/ir`,
/// by id. Half-written or foreign folders are skipped.
pub fn list_installed(root: &Path) -> Vec<InstalledPlugin> {
    let mut found: Vec<InstalledPlugin> = [NAM_FOLDER, IR_FOLDER]
        .iter()
        .filter_map(|folder| std::fs::read_dir(root.join(folder)).ok())
        .flatten()
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
                    updated_at: read_updated_at(&dir),
                    dir,
                    manifest,
                },
            )
        })
        .collect();
    found.sort_by(|a, b| a.plugin_id.cmp(&b.plugin_id));
    found
}

/// Deletes the package at `dir`. Only a package folder inside `root` (the
/// plugins folder) is accepted, never `root` itself or a kind folder.
pub fn remove_package(root: &Path, dir: &Path) -> Result<(), InstallError> {
    let refused = || InstallError::NotInPluginsFolder(dir.display().to_string());
    if !dir.is_dir() {
        return Err(InstallError::NotInstalled(dir.display().to_string()));
    }
    let (Ok(root), Ok(target)) = (root.canonicalize(), dir.canonicalize()) else {
        return Err(refused());
    };
    if target == root || !target.starts_with(&root) || !target.join(MANIFEST_FILE).is_file() {
        return Err(refused());
    }
    std::fs::remove_dir_all(&target)?;
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
