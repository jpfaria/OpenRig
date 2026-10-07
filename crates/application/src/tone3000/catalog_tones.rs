//! Responsibility: finds the catalog plugins that came from a TONE3000 tone.
//!
//! A plugin names its tone in `sources` or `homepage`
//! (`https://www.tone3000.com/tones/<id>`). The browser uses this to show a
//! tone the user already has, wherever its package lives (#879).

use plugin_loader::manifest::{Backend, PluginManifest};

use super::installed::is_plugin_id;
use crate::tone3000_state::Tone3000InstalledEntry;

const TONE_URL: &str = "tone3000.com/tones/";

/// `https://www.tone3000.com/tones/52557` → `52557`.
pub fn source_tone_id(url: &str) -> Option<u64> {
    let (_, rest) = url.split_once(TONE_URL)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Every TONE3000 tone a manifest points at, in order, without repeats.
pub fn manifest_tone_ids(manifest: &PluginManifest) -> Vec<u64> {
    let mut ids = Vec::new();
    let urls = manifest
        .sources
        .iter()
        .flatten()
        .chain(manifest.homepage.iter());
    for id in urls.filter_map(|url| source_tone_id(url)) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// One listed package for `manifest`; `removable` when the browser owns it.
pub fn installed_entry(manifest: &PluginManifest, removable: bool) -> Tone3000InstalledEntry {
    let captures = match &manifest.backend {
        Backend::Nam { captures, .. } | Backend::Ir { captures, .. } => captures.len(),
        _ => 0,
    };
    Tone3000InstalledEntry {
        plugin_id: manifest.id.clone(),
        tone_ids: manifest_tone_ids(manifest),
        display_name: manifest.display_name.clone(),
        block_type: manifest.block_type,
        architecture: manifest.architecture,
        captures,
        removable,
        updated_at: None,
    }
}

/// The catalog plugins that came from TONE3000, sorted by name. The
/// browser's own `tone3000_*` packages are left out: they are listed from
/// their folder.
pub fn catalog_entries<'a>(
    manifests: impl IntoIterator<Item = &'a PluginManifest>,
) -> Vec<Tone3000InstalledEntry> {
    let mut found: Vec<Tone3000InstalledEntry> = manifests
        .into_iter()
        .filter(|m| !is_plugin_id(&m.id))
        .filter(|m| !manifest_tone_ids(m).is_empty())
        .map(|m| installed_entry(m, false))
        .collect();
    found.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    found
}

/// [`catalog_entries`] over the loaded plugin catalog.
pub fn loaded_catalog_entries() -> Vec<Tone3000InstalledEntry> {
    catalog_entries(
        plugin_loader::registry::packages()
            .iter()
            .map(|p| &p.manifest),
    )
}
