//! Responsibility: tells which owned folder a plugin package lives in.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use plugin_loader::LoadedPackage;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The folders whose plugins the user owns. `None` when the session has not
/// attached that folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginRoots {
    pub plugins_folder: Option<PathBuf>,
    pub tone3000: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginOrigin {
    PluginsFolder,
    Tone3000,
}

impl PluginRoots {
    /// Where the package at `package_root` comes from, or `None` when it is
    /// not the user's (a plugin the app ships).
    pub fn origin_of(&self, package_root: &Path) -> Option<PluginOrigin> {
        let inside =
            |root: &Option<PathBuf>| root.as_deref().is_some_and(|r| is_inside(package_root, r));
        if inside(&self.tone3000) {
            Some(PluginOrigin::Tone3000)
        } else if inside(&self.plugins_folder) {
            Some(PluginOrigin::PluginsFolder)
        } else {
            None
        }
    }

    /// The loaded package `plugin_id` and its origin, when the user owns it.
    pub fn owned(&self, plugin_id: &str) -> Result<(&'static LoadedPackage, PluginOrigin)> {
        let package = plugin_loader::registry::find(plugin_id)
            .ok_or_else(|| anyhow!("plugin `{plugin_id}` is not in the catalog"))?;
        let origin = self
            .origin_of(&package.root)
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
