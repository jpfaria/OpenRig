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

use std::sync::OnceLock;
use crate::discovery::{scan_system_vst3, Vst3PluginInfo};
use block_core::ModelVisualData;

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

/// Leak a `String` into a `&'static str`.
///
/// Safe because the catalog is initialised once and never dropped.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Build the stable model ID for a discovered plugin.
pub fn make_model_id(info: &Vst3PluginInfo) -> String {
    let bundle_stem = info
        .bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let class_name = info.name.replace(' ', "_");
    format!("vst3:{}:{}", bundle_stem, class_name)
}

/// Initialise the VST3 catalog by scanning standard system paths.
///
/// Must be called once at startup before `vst3_catalog()` is used.
/// Subsequent calls are no-ops (the `OnceLock` prevents re-initialisation).
///
/// `sample_rate` is used to instantiate plugins for parameter enumeration.
pub fn init_vst3_catalog(sample_rate: f64) {
    CATALOG.get_or_init(|| {
        let infos = scan_system_vst3(sample_rate);
        log::info!("VST3 catalog: discovered {} plugins", infos.len());
        infos
            .into_iter()
            .map(|info| {
                let model_id = leak(make_model_id(&info));
                let display_name = leak(info.name.clone());
                let brand = leak(info.vendor.clone());
                let category = leak(info.category.clone());
                Vst3CatalogEntry { model_id, display_name, brand, category, info }
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
    })
}
