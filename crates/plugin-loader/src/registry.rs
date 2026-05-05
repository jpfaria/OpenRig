//! Process-wide registry of every plugin known at runtime.
//!
//! Two ways a plugin enters the registry:
//!
//! - **Disk packages** — `init(plugins_root)` runs `discover()` once at
//!   startup and pushes one [`LoadedPackage`] per `manifest.yaml` found.
//! - **Native plugins** — each `block-*` crate calls
//!   [`register_native`] for each compiled-in DSP model, supplying a
//!   synthesized [`PluginManifest`] (with `Backend::Native { runtime_id }`)
//!   plus the runtime fn pointers that go into
//!   [`crate::native_runtimes`].
//!
//! Native registration must happen **before** [`init`] is called. After
//! `init` the registry is frozen ([`OnceLock`]) and read-only — every
//! consumer (GUI, engine) reaches it via [`packages`], [`packages_for`],
//! or [`find`] without threading the path through every call site.
//!
//! Issue: #287

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::discover::{discover, LoadedPackage};
use crate::manifest::{Backend, BlockType, PluginManifest};
use crate::native_runtimes::{self, NativeRuntime};

static REGISTRY: OnceLock<Vec<LoadedPackage>> = OnceLock::new();
static PENDING_NATIVES: Mutex<Vec<LoadedPackage>> = Mutex::new(Vec::new());

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
    PENDING_NATIVES
        .lock()
        .expect("PENDING_NATIVES poisoned")
        .push(entry);
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
        screenshot: None,
        brand_logo: None,
        license: Some("internal".to_string()),
        homepage: None,
        sources: None,
        block_type,
        backend: Backend::Native {
            runtime_id: id.to_string(),
        },
    };
    register_native(manifest, runtime);
}

/// Discover every package under `plugins_root`, merge with previously
/// registered natives, and freeze the catalog.
///
/// Idempotent: a second call is a no-op (the catalog never changes
/// during a process lifetime). Per-package failures are dropped but
/// logged to stderr so the boot path can't surface them per-block.
pub fn init(plugins_root: &Path) {
    if REGISTRY.get().is_some() {
        return;
    }
    let mut loaded: Vec<LoadedPackage> = PENDING_NATIVES
        .lock()
        .expect("PENDING_NATIVES poisoned")
        .drain(..)
        .collect();
    match discover(plugins_root) {
        Ok(results) => {
            for result in results {
                match result {
                    Ok(package) => loaded.push(package),
                    Err(error) => eprintln!("plugin-loader: skipping package: {error}"),
                }
            }
        }
        Err(error) => {
            eprintln!(
                "plugin-loader: cannot read plugins_root `{}`: {error}",
                plugins_root.display()
            );
        }
    }
    let _ = REGISTRY.set(loaded);
}

/// Every plugin currently registered (natives + disk packages). Empty
/// until [`init`] runs.
pub fn packages() -> &'static [LoadedPackage] {
    match REGISTRY.get() {
        Some(packages) => packages,
        None => &[],
    }
}

/// Plugins whose manifest declares `block_type`. Returned in registration
/// order (natives first, then disk packages alphabetically by directory).
pub fn packages_for(block_type: BlockType) -> Vec<&'static LoadedPackage> {
    packages()
        .iter()
        .filter(|p| p.manifest.block_type == block_type)
        .collect()
}

/// Look up a single plugin by manifest id (`p.manifest.id`).
pub fn find(model_id: &str) -> Option<&'static LoadedPackage> {
    packages().iter().find(|p| p.manifest.id == model_id)
}

/// Count of natives + disk packages currently in the catalog.
pub fn len() -> usize {
    packages().len()
}

/// Count of just the native plugins (entries whose backend is `Native`).
pub fn native_count() -> usize {
    packages()
        .iter()
        .filter(|p| matches!(p.manifest.backend, Backend::Native { .. }))
        .count()
}
