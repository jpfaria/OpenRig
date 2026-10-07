//! Responsibility: lists the plugins the user owns as library entries.

use plugin_loader::manifest::{Backend, BlockType, NamArchitecture};
use plugin_loader::version_store::version_numbers;
use serde::Serialize;

use super::roots::{PluginOrigin, PluginRoots};
use crate::tone3000::catalog_tones::manifest_tone_ids;
use crate::tone3000::source_stamp::read_updated_at;

/// One row of the plugin catalog window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginLibraryEntry {
    pub plugin_id: String,
    pub display_name: String,
    pub brand: Option<String>,
    pub block_type: BlockType,
    /// How it plays: `nam`, `ir`, `lv2`, `vst3`, ...
    pub backend: String,
    pub architecture: Option<NamArchitecture>,
    pub origin: PluginOrigin,
    pub captures: usize,
    /// A capture plugin, whose parameters the editor can change.
    pub editable: bool,
    /// The TONE3000 tones it came from.
    pub tone_ids: Vec<u64>,
    /// The TONE3000 tone version it holds, when known.
    pub updated_at: Option<String>,
    /// The kept versions of its parameters, oldest first.
    pub versions: Vec<u32>,
}

/// Every loaded plugin under one of `roots`, by display name.
pub fn library_entries(roots: &PluginRoots) -> Vec<PluginLibraryEntry> {
    let mut entries: Vec<PluginLibraryEntry> = plugin_loader::registry::packages()
        .iter()
        .filter_map(|package| {
            let origin = roots.origin_of(package)?;
            let manifest = &package.manifest;
            let captures = match &manifest.backend {
                Backend::Nam { captures, .. } | Backend::Ir { captures, .. } => {
                    Some(captures.len())
                }
                _ => None,
            };
            Some(PluginLibraryEntry {
                plugin_id: manifest.id.clone(),
                display_name: manifest.display_name.clone(),
                brand: manifest.brand.clone(),
                block_type: manifest.block_type,
                backend: backend_name(&manifest.backend),
                architecture: manifest.architecture,
                origin,
                captures: captures.unwrap_or(0),
                editable: captures.is_some(),
                tone_ids: manifest_tone_ids(manifest),
                updated_at: read_updated_at(&package.root),
                versions: version_numbers(&package.root),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
            .then_with(|| a.plugin_id.cmp(&b.plugin_id))
    });
    entries
}

/// The manifest's own spelling of its backend.
fn backend_name(backend: &Backend) -> String {
    serde_json::to_value(backend)
        .ok()
        .and_then(|v| {
            v.get("backend")
                .and_then(|b| b.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}
