//! Responsibility: resolves a VST3 block model id to its catalog entry.

use std::path::Path;

use plugin_loader::manifest::Backend;
use vst3_host::Vst3CatalogEntry;

/// The catalog entry a VST3 block model names. Its `model_id`
/// (`vst3:{bundle}:{class}`) is the key every VST3 cache uses.
///
/// Accepts the catalog id itself and — because `openrig://plugins` lists a
/// VST3 package by its manifest id (`vst3_room_reverb`) — that id too: the
/// package's bundle is looked up in the catalog (#938).
pub fn vst3_catalog_entry(model: &str) -> Option<&'static Vst3CatalogEntry> {
    if let Some(entry) = vst3_host::find_vst3_plugin(model) {
        return Some(entry);
    }
    let package = plugin_loader::registry::find(model)?;
    let Backend::Vst3 { bundle, .. } = &package.manifest.backend else {
        return None;
    };
    entry_for_bundle(vst3_host::vst3_catalog(), &package.root.join(bundle))
}

/// The catalog entry discovered from `bundle`. Paths are compared after
/// resolving `.`/`..` and links, since the catalog and the package
/// registry reach the same bundle from separately joined roots.
pub(crate) fn entry_for_bundle<'a>(
    catalog: &'a [Vst3CatalogEntry],
    bundle: &Path,
) -> Option<&'a Vst3CatalogEntry> {
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let wanted = canonical(bundle);
    catalog
        .iter()
        .find(|entry| canonical(&entry.info.bundle_path) == wanted)
}

#[cfg(test)]
#[path = "vst3_model_id_tests.rs"]
mod tests;
