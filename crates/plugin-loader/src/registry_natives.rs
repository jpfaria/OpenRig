//! Responsibility: registers a native runtime under the catalog.

use crate::discover::LoadedPackage;
use crate::manifest::{Backend, BlockType, PluginManifest};
use crate::native_runtimes::{self, NativeRuntime};
use crate::registry::NATIVES;
use std::path::PathBuf;

/// Add a native plugin to the catalog. Called by each `block-*` crate at
/// startup, once per compiled-in DSP model.
///
/// `manifest` describes the plugin in the same shape used by disk
/// packages — id, display_name, brand, block_type, parameters — but with
/// `backend: Backend::Native { runtime_id }`. `runtime` carries the fn
/// pointers (schema, validate, build) used at instantiation time;
/// [`native_runtimes::register`] indexes them by the same `runtime_id`.
///
/// Panics if `manifest.backend` is not `Backend::Native`.
pub fn register_native(manifest: PluginManifest, runtime: NativeRuntime) {
    let runtime_id = match &manifest.backend {
        Backend::Native { runtime_id } => runtime_id.clone(),
        other => panic!("register_native expects Backend::Native, got {other:?}"),
    };
    native_runtimes::register(&runtime_id, runtime);
    let entry = LoadedPackage {
        root: PathBuf::new(),
        manifest,
    };
    NATIVES.lock().expect("NATIVES poisoned").push(entry);
}

/// Convenience over [`register_native`]: synthesizes the [`PluginManifest`]
/// for a native model from its bare metadata, so each `block-*` crate
/// doesn't have to re-spell the full manifest struct per model.
///
/// `runtime_id` doubles as the manifest `id` — natives are unique by id
/// across the catalog, so there's no need for a separate key.
pub fn register_native_simple(
    id: &str,
    display_name: &str,
    brand: Option<&str>,
    block_type: BlockType,
    runtime: NativeRuntime,
) {
    let manifest = PluginManifest {
        manifest_version: 1,
        id: id.to_string(),
        display_name: display_name.to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: brand.map(str::to_string),
        thumbnail: None,
        photo: None,
        output_gain_db: None,
        noise_gate: None,
        screenshot: None,
        brand_logo: None,
        license: Some("internal".to_string()),
        homepage: None,
        sources: None,
        architecture: None,
        block_type,
        backend: Backend::Native {
            runtime_id: id.to_string(),
        },
    };
    register_native(manifest, runtime);
}
