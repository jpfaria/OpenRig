//! Catalog `type_label` derivation for disk-backed plugin packages.
//!
//! Lifted out of `catalog.rs` so that file stays under its size cap (#276).
//! These functions own the badge string a disk package shows in the GUI
//! (block picker, hover tooltip, plugin-info window, block-editor header).

use plugin_loader::manifest::{Backend, PluginManifest};

/// Base backend tag for a disk package (`"NATIVE"`, `"NAM"`, `"IR"`,
/// `"LV2"`, `"VST3"`).
pub(crate) fn backend_label_for(backend: &Backend) -> &'static str {
    match backend {
        Backend::Native { .. } => "NATIVE",
        Backend::Nam { .. } => "NAM",
        Backend::Ir { .. } => "IR",
        Backend::Lv2 { .. } => "LV2",
        Backend::Vst3 { .. } => "VST3",
    }
}

/// Catalog `type_label` for a disk-package, e.g. `"NAM"`, `"NAM/A1"`,
/// `"NAM/A2"`, `"IR"`, `"LV2"`.
///
/// NAM packages append their declared architecture (issue #650) so the
/// catalog can show NAM/A1 vs NAM/A2 **without opening any `.nam`**. The
/// architecture field is only meaningful for NAM; other backends ignore it.
pub(crate) fn package_type_label(manifest: &PluginManifest) -> String {
    let base = backend_label_for(&manifest.backend);
    match (&manifest.backend, manifest.architecture) {
        (Backend::Nam { .. }, Some(arch)) => format!("{base}/{}", arch.as_str()),
        _ => base.to_string(),
    }
}

#[cfg(test)]
#[path = "catalog_label_tests.rs"]
mod tests;
