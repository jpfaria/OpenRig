//! Responsibility: holds the VST3 plugins discovery found.
//! Runtime catalog of dynamically discovered VST3 plugins.
//!
//! Call `init_vst3_catalog()` once at application startup (after the audio
//! device is known, so sample_rate is available). All subsequent calls to
//! `vst3_catalog()` / `find_vst3_plugin()` are lock-free reads.
//!
//! Model IDs for discovered plugins follow the scheme:
//!   `vst3:{bundle_stem}:{class_name}`
//! where `bundle_stem` is the `.vst3` directory without extension and
//! `class_name` is the plugin's display name with spaces replaced by `_`.
//! This scheme is stable as long as the plugin is installed at the same path.

use crate::discovery::{scan_system_vst3, scan_vst3_dirs, Vst3PluginInfo};
use block_core::ModelVisualData;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub use crate::catalog_params::catalog_params;
#[cfg(test)]
pub(crate) use crate::catalog_params::resolve_params;
pub(crate) use crate::plugin_uid_cache::leak;
pub use crate::plugin_uid_cache::make_model_id;
pub use crate::plugin_uid_cache::resolve_uid_for_model;

/// A discovered VST3 plugin with its stable runtime model ID.
#[derive(Debug, Clone)]
pub struct Vst3CatalogEntry {
    /// Stable model ID: `vst3:{bundle_stem}:{class_name}`.
    pub model_id: &'static str,
    /// Human-readable name (plugin's class name).
    pub display_name: &'static str,
    /// Vendor / brand name.
    pub brand: &'static str,
    /// VST3 audio category label (e.g. "Fx|Reverb").
    pub category: &'static str,
    /// The underlying discovery info needed to instantiate the plugin.
    pub info: Vst3PluginInfo,
}

static CATALOG: OnceLock<Vec<Vst3CatalogEntry>> = OnceLock::new();

/// Cache for lazily-resolved UIDs: bundle_path → class_name → uid.
pub(crate) static UID_CACHE: OnceLock<Mutex<HashMap<PathBuf, HashMap<String, [u8; 16]>>>> =
    OnceLock::new();

/// Initialise the VST3 catalog by scanning standard system paths.
///
/// Must be called once at startup before `vst3_catalog()` is used.
/// Subsequent calls are no-ops (the `OnceLock` prevents re-initialisation).
///
/// Uses light scanning (no plugin instantiation), so it is safe even for
/// complex commercial plugins that might crash on full initialisation.
/// `sample_rate` is kept for API compatibility but is no longer used here.
///
/// `extra_dirs` are scanned alongside the standard system paths (issue #776):
/// the caller passes the OpenRig plugins folder(s) so catalog VST3 bundles
/// (`<plugins_root>/vst3/<id>/bundles/`) join the same catalog as
/// system-installed plugins — same model-ID scheme, same block kind, same
/// native editor. A bundle discovered in both places (same `model_id`) is
/// kept once.
pub fn init_vst3_catalog(sample_rate: f64, extra_dirs: &[PathBuf]) {
    CATALOG.get_or_init(|| {
        let mut infos = scan_system_vst3(sample_rate); // sample_rate unused (light scan)
        infos.extend(scan_vst3_dirs(extra_dirs));
        log::info!("VST3 catalog: discovered {} plugins", infos.len());
        let mut seen: HashSet<String> = HashSet::new();
        infos
            .into_iter()
            .filter_map(|info| {
                let id = make_model_id(&info);
                if !seen.insert(id.clone()) {
                    return None; // same bundle found in a system path and a plugins root
                }
                let model_id = leak(id);
                let display_name = leak(info.name.clone());
                let brand = leak(info.vendor.clone());
                let category = leak(info.category.clone());
                Some(Vst3CatalogEntry {
                    model_id,
                    display_name,
                    brand,
                    category,
                    info,
                })
            })
            .collect()
    });
}

/// Return a reference to the global VST3 catalog.
///
/// Returns an empty slice if `init_vst3_catalog()` has not been called yet.
pub fn vst3_catalog() -> &'static [Vst3CatalogEntry] {
    CATALOG.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Look up a catalog entry by its model ID.
pub fn find_vst3_plugin(model_id: &str) -> Option<&'static Vst3CatalogEntry> {
    vst3_catalog().iter().find(|e| e.model_id == model_id)
}

pub(crate) static PARAM_CACHE: OnceLock<Mutex<HashMap<String, Vec<crate::host::Vst3ParamInfo>>>> =
    OnceLock::new();

/// Return all model IDs in the catalog.
///
/// The returned slice is valid for the process lifetime.
pub fn vst3_model_ids() -> Vec<&'static str> {
    vst3_catalog().iter().map(|e| e.model_id).collect()
}

/// Return `ModelVisualData` for a given model ID, if it exists in the catalog.
pub fn vst3_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let entry = find_vst3_plugin(model_id)?;
    Some(ModelVisualData {
        brand: entry.brand,
        type_label: "VST3",
        supported_instruments: block_core::ALL_INSTRUMENTS,
        knob_layout: &[],
        thumbnail_path: None,
        available: true,
    })
}

#[cfg(test)]
mod resolve_params_tests {
    use super::resolve_params;

    #[test]
    fn empty_live_read_falls_back_to_loader() {
        // A registered-but-empty live read must NOT shadow the throw-away load,
        // or the compact view shows a VST3 block with no params (#780).
        let loaded = resolve_params(Some(Vec::<i32>::new()), || vec![1, 2, 3]);
        assert_eq!(loaded, vec![1, 2, 3], "empty live → fall back to loader");
    }

    #[test]
    fn non_empty_live_read_wins() {
        let live = resolve_params(Some(vec![9]), || {
            panic!("must not load when live has params")
        });
        assert_eq!(live, vec![9]);
    }

    #[test]
    fn no_live_uses_loader() {
        let loaded = resolve_params(None, || vec![7]);
        assert_eq!(loaded, vec![7]);
    }
}
