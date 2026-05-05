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

use crate::discovery::{scan_system_vst3, Vst3PluginInfo};
use crate::host::Vst3Plugin;
use block_core::ModelVisualData;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

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
static UID_CACHE: OnceLock<Mutex<HashMap<PathBuf, HashMap<String, [u8; 16]>>>> = OnceLock::new();

fn uid_cache() -> &'static Mutex<HashMap<PathBuf, HashMap<String, [u8; 16]>>> {
    UID_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

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
/// Uses light scanning (no plugin instantiation), so it is safe even for
/// complex commercial plugins that might crash on full initialisation.
/// `sample_rate` is kept for API compatibility but is no longer used here.
pub fn init_vst3_catalog(sample_rate: f64) {
    CATALOG.get_or_init(|| {
        let infos = scan_system_vst3(sample_rate); // sample_rate unused (light scan)
        log::info!("VST3 catalog: discovered {} plugins", infos.len());
        infos
            .into_iter()
            .map(|info| {
                let model_id = leak(make_model_id(&info));
                let display_name = leak(info.name.clone());
                let brand = leak(info.vendor.clone());
                let category = leak(info.category.clone());
                Vst3CatalogEntry {
                    model_id,
                    display_name,
                    brand,
                    category,
                    info,
                }
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
        thumbnail_path: None,
        available: true,
    })
}

/// Resolve the UID for a catalog entry.
///
/// If the UID was already known from `moduleinfo.json` (uid != [0;16]), returns it
/// immediately. Otherwise, performs a lazy `enumerate_classes()` call to discover
/// the UID from the plugin factory, caches the result, and returns it.
///
/// **Warning**: For plugins without `moduleinfo.json` (e.g. ValhallaSupermassive,
/// Guitar Rig 7) this will call `dlopen()` on the plugin dylib. Most plugins are
/// safe, but some complex commercial plugins may deadlock or crash the process.
pub fn resolve_uid_for_model(model_id: &str) -> anyhow::Result<[u8; 16]> {
    let entry = find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    // Fast path: UID already known from moduleinfo.json.
    if entry.info.uid != [0u8; 16] {
        return Ok(entry.info.uid);
    }

    let bundle_path = &entry.info.bundle_path;
    let class_name = entry.display_name;

    // Check cache first.
    {
        let cache = uid_cache().lock().unwrap();
        if let Some(by_class) = cache.get(bundle_path) {
            if let Some(&uid) = by_class.get(class_name) {
                return Ok(uid);
            }
        }
    }

    // Lazy resolution via enumerate_classes (performs dlopen).
    log::info!(
        "VST3: lazy UID resolution for '{}' in {}",
        class_name,
        bundle_path.display()
    );
    let (_lib, classes) = Vst3Plugin::enumerate_classes(bundle_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to enumerate classes in {}: {}",
            bundle_path.display(),
            e
        )
    })?;

    // Cache all classes from this bundle.
    let mut cache = uid_cache().lock().unwrap();
    let by_class = cache.entry(bundle_path.clone()).or_default();
    for cls in &classes {
        by_class.insert(cls.name.clone(), cls.uid);
    }

    // Find the matching class.
    classes
        .iter()
        .find(|c| c.name == class_name)
        .map(|c| c.uid)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "class '{}' not found in bundle {} (found: {})",
                class_name,
                bundle_path.display(),
                classes
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}
