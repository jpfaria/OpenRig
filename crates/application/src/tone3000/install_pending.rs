//! Responsibility: holds a downloaded tone whose parameters the user still has to name.
//!
//! The captures, their levels and the inferred manifest wait in a hidden
//! `.pending-<id>` folder beside the package's place. The manifest is kept
//! as `pending.yaml`, so the loader never reads the folder as a package.
//! Naming the parameters moves the folder into place and writes the real
//! manifest last; dropping it removes the folder.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use plugin_loader::manifest::PluginManifest;
use project::block::capture_engine_params::capture_engine_parameter_names;
use project::block::grid_version_follow::capture_grid;
use serde::Serialize;

use super::axes::PRESET_AXIS;
use super::install_error::InstallError;
use super::installed::{InstalledPlugin, MANIFEST_FILE};
use super::source_stamp::read_updated_at;
use crate::plugin_library::grid_apply::apply_grid;
use crate::plugin_library::grid_read::read_grid;
use crate::plugin_library::manifest_grid::replace_grid;
use crate::plugin_library::EditorGrid;

const PENDING_FILE: &str = "pending.yaml";

/// One tone waiting for names: where it waits and the grid the user edits.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingInstall {
    pub tone_id: u64,
    pub plugin_id: String,
    /// The hidden folder holding the downloaded captures.
    pub dir: PathBuf,
    pub grid: EditorGrid,
}

/// The waiting folder of package `id` inside a kind folder.
pub fn pending_dir(kind_folder: &Path, id: &str) -> PathBuf {
    kind_folder.join(format!(".pending-{id}"))
}

/// The inference could not read some capture names, so it fell back to one
/// parameter of raw words that only the user can name.
pub fn needs_names(manifest: &PluginManifest) -> bool {
    capture_grid(manifest)
        .is_some_and(|(parameters, _)| parameters.iter().any(|p| p.name == PRESET_AXIS))
}

/// Moves a complete staging folder aside to wait for names.
pub fn park(
    tone_id: u64,
    staging: &Path,
    pending: &Path,
    manifest: &PluginManifest,
) -> Result<PendingInstall, InstallError> {
    let yaml =
        serde_yaml::to_string(manifest).map_err(|e| InstallError::Manifest(e.to_string()))?;
    std::fs::write(staging.join(PENDING_FILE), yaml)?;
    let _ = std::fs::remove_dir_all(pending);
    std::fs::rename(staging, pending)?;
    let (parameters, captures) =
        capture_grid(manifest).ok_or_else(|| InstallError::Manifest("no captures".into()))?;
    Ok(PendingInstall {
        tone_id,
        plugin_id: manifest.id.clone(),
        dir: pending.to_path_buf(),
        grid: read_grid(pending, parameters, captures),
    })
}

/// Applies `grid` and puts the package in place. A grid that cannot pick
/// every capture leaves the tone waiting, untouched.
pub fn finish(pending: &PendingInstall, grid: &EditorGrid) -> Result<InstalledPlugin> {
    let text = std::fs::read_to_string(pending.dir.join(PENDING_FILE))
        .with_context(|| format!("reading {}", pending.dir.display()))?;
    let inferred: PluginManifest = serde_yaml::from_str(&text)?;
    let (parameters, captures) =
        capture_grid(&inferred).ok_or_else(|| anyhow!("the tone has no captures"))?;
    let (parameters, captures) = apply_grid(
        grid,
        parameters,
        captures,
        &capture_engine_parameter_names(),
    )?;
    let manifest = replace_grid(&inferred, parameters, captures)
        .ok_or_else(|| anyhow!("the tone has no captures"))?;
    plugin_loader::validate_manifest(&manifest).map_err(|e| anyhow!("{e}"))?;
    let yaml = serde_yaml::to_string(&manifest)?;
    let Some(folder) = pending.dir.parent() else {
        bail!("{} has no parent folder", pending.dir.display());
    };
    let dir = folder.join(&pending.plugin_id);
    if dir.exists() {
        bail!("`{}` is already installed", pending.plugin_id);
    }
    std::fs::rename(&pending.dir, &dir)?;
    let _ = std::fs::remove_file(dir.join(PENDING_FILE));
    let tmp = dir.join(format!(".{MANIFEST_FILE}.tmp"));
    std::fs::write(&tmp, yaml)?;
    std::fs::rename(&tmp, dir.join(MANIFEST_FILE))?;
    Ok(InstalledPlugin {
        plugin_id: pending.plugin_id.clone(),
        updated_at: read_updated_at(&dir),
        dir,
        manifest,
    })
}

/// Removes the waiting folder.
pub fn drop_pending(pending: &PendingInstall) -> Result<()> {
    std::fs::remove_dir_all(&pending.dir)
        .with_context(|| format!("removing {}", pending.dir.display()))
}
