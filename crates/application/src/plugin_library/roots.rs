//! Responsibility: classifies a loaded plugin package by who put it in the plugins folder.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use plugin_loader::LoadedPackage;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tone3000::catalog_tones::manifest_tone_ids;
use crate::tone3000::installed::is_plugin_id;

/// The folder whose plugins the user owns: the plugins folder, where
/// TONE3000 installs go too. `None` when the session has not attached it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginRoots {
    pub plugins_folder: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginOrigin {
    PluginsFolder,
    Tone3000,
}

impl PluginRoots {
    /// Where `package` comes from, or `None` when it is not the user's (a
    /// plugin the app ships). A package that names a TONE3000 tone, or that
    /// the TONE3000 browser installed, came from TONE3000.
    pub fn origin_of(&self, package: &LoadedPackage) -> Option<PluginOrigin> {
        let folder = self.plugins_folder.as_deref()?;
        if !is_inside(&package.root, folder) {
            return None;
        }
        let manifest = &package.manifest;
        if is_plugin_id(&manifest.id) || !manifest_tone_ids(manifest).is_empty() {
            Some(PluginOrigin::Tone3000)
        } else {
            Some(PluginOrigin::PluginsFolder)
        }
    }

    /// The loaded package `plugin_id` and its origin, when the user owns it.
    pub fn owned(&self, plugin_id: &str) -> Result<(&'static LoadedPackage, PluginOrigin)> {
        let package = plugin_loader::registry::find(plugin_id)
            .ok_or_else(|| anyhow!("plugin `{plugin_id}` is not in the catalog"))?;
        let origin = self
            .origin_of(package)
            .ok_or_else(|| anyhow!("`{plugin_id}` ships with the app and cannot be changed"))?;
        Ok((package, origin))
    }
}

/// `path` is `root` or below it, compared as written and, failing that,
/// with links resolved (a plugins folder is often a link).
fn is_inside(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(path), Ok(root)) => path.starts_with(root),
        _ => false,
    }
}
