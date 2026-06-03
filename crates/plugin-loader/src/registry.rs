//! Process-wide registry of every plugin known at runtime.
//!
//! Two ways a plugin enters the registry:
//!
//! - **Disk packages** — `init(plugins_root)` (or `init_many`) runs
//!   `discover()` once at startup and pushes one [`LoadedPackage`] per
//!   `manifest.yaml` found. `reload(plugins_roots)` re-runs the disk
//!   scan after a new pack lands on disk so the running process picks
//!   it up without a restart (issue #561).
//! - **Native plugins** — each `block-*` crate calls
//!   [`register_native`] for each compiled-in DSP model, supplying a
//!   synthesized [`PluginManifest`] (with `Backend::Native { runtime_id }`)
//!   plus the runtime fn pointers that go into
//!   [`crate::native_runtimes`].
//!
//! Native registration happens **before** [`init`] / [`init_many`] is
//! called. The native list is kept in a separate static so [`reload`]
//! can rebuild the disk side without losing the natives (they cannot
//! be re-discovered — they have no manifest on disk). Every call to
//! `reload` re-scans the disk roots and atomically swaps the public
//! `&'static [LoadedPackage]` slice; old references taken before the
//! swap stay valid (the previous slice is leaked, not freed), so any
//! cached `&'static LoadedPackage` survives the reload.
//!
//! Issues: #287, #561

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};

use crate::discover::{discover, LoadedPackage};
use crate::manifest::{Backend, BlockType, PluginManifest};
use crate::native_runtimes::{self, NativeRuntime};

/// Persistent list of natives. Populated once at startup by
/// `block-*::register_natives`; never drained. [`reload`] reads this
/// each time it rebuilds the public registry so the natives are not
/// lost when re-scanning disk roots.
static NATIVES: Mutex<Vec<LoadedPackage>> = Mutex::new(Vec::new());

/// The currently published catalog. Always points at a leaked, immutable
/// slice — readers get `&'static` references that survive subsequent
/// reloads (the previous slice is intentionally not freed).
static REGISTRY: RwLock<&'static [LoadedPackage]> = RwLock::new(&[]);

/// Tracks whether [`init_many`] has already taken over publishing the
/// catalog. Subsequent `init_many` calls are no-ops (matches the
/// pre-#561 `OnceLock` semantics); [`reload`] bypasses this flag.
static REGISTRY_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Counts emitted by [`reload`] and surfaced via
/// `Event::PluginCatalogReloaded` (issue #561) so adapters (GUI toast,
/// MCP, gRPC) can show the user what changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReloadStats {
    /// Natives currently in the catalog (always >= what `register_native`
    /// pushed; never decreases across reloads).
    pub native_count: usize,
    /// Disk packages discovered under `plugins_roots` on this reload.
    pub disk_count: usize,
    /// `native_count + disk_count`.
    pub total_count: usize,
}

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

/// Discover every package under `plugins_root`, merge with previously
/// registered natives, and publish the catalog.
///
/// Idempotent: a second call is a no-op (matches the pre-#561 contract
/// for boot wiring). Use [`reload`] to force a rescan.
///
/// Backwards-compatible single-root entry point. Equivalent to
/// `init_many(&[plugins_root])`.
pub fn init(plugins_root: &Path) {
    init_many(std::slice::from_ref(&plugins_root.to_path_buf()));
}

/// Multi-root variant — scans every directory in `plugins_roots`,
/// merging results into a single registry. Use this to expose both
/// the bundled (read-only, ships with the installer) and the user
/// (writable, user-installed) plugin trees. Missing/empty directories
/// are skipped silently — only hard read errors are logged.
///
/// First call wins; subsequent calls are no-ops. Per-#561, [`reload`]
/// is now the source of truth for rebuilding the catalog — this
/// function is a thin "first-time-only" wrapper around it.
pub fn init_many(plugins_roots: &[std::path::PathBuf]) {
    if REGISTRY_INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = reload(plugins_roots);
}

/// Re-scan every directory in `plugins_roots`, rebuild the catalog,
/// and atomically swap it in. Natives are preserved (they cannot be
/// rediscovered — they have no manifest on disk).
///
/// Used by `Command::ReloadPluginCatalog` (issue #561) so the running
/// process picks up freshly installed plugins without a restart. Also
/// adopted by [`init_many`] as the single source of truth for "build
/// the catalog from these roots".
///
/// Returns the new counts so adapters can surface them to the user
/// (GUI toast, MCP tool response). Old `&'static LoadedPackage`
/// references handed out before the reload remain valid — the
/// previous slice is intentionally leaked so cached references can't
/// dangle.
pub fn reload(plugins_roots: &[std::path::PathBuf]) -> ReloadStats {
    let natives = NATIVES.lock().expect("NATIVES poisoned").clone();
    let native_count = natives.len();
    let mut loaded = natives;
    // Natives always win — their runtime fn pointers are compiled in and
    // have no manifest on disk to override them.
    let native_ids: std::collections::HashSet<String> =
        loaded.iter().map(|e| e.manifest.id.clone()).collect();
    // Among disk roots, a LATER root overrides an EARLIER one on id
    // collision (issue #542): `init_many(&[bundled_root, user_root])`
    // passes the read-only bundled tree first and the user's writable
    // `plugins_path` second, so the user's copy must win. Otherwise a
    // bundled IR cab shipped with an uncalibrated `output_gain_db: 0.0`
    // shadows the user's calibrated copy → the convolver runs raw →
    // ~+18 dB hot → "estourado". Maps each disk id to its slot so a
    // later root replaces in place (keeping registration order stable).
    let mut disk_slot: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for root in plugins_roots {
        if !root.is_dir() {
            continue;
        }
        match discover(root) {
            Ok(results) => {
                for result in results {
                    match result {
                        Ok(package) => {
                            let id = package.manifest.id.clone();
                            if native_ids.contains(&id) {
                                // A native with this id already won.
                            } else if let Some(&slot) = disk_slot.get(&id) {
                                // Later root overrides the earlier one.
                                loaded[slot] = package;
                            } else {
                                disk_slot.insert(id, loaded.len());
                                loaded.push(package);
                            }
                        }
                        Err(error) => {
                            eprintln!("plugin-loader: skipping package: {error}")
                        }
                    }
                }
            }
            Err(error) => {
                eprintln!(
                    "plugin-loader: cannot read plugins_root `{}`: {error}",
                    root.display()
                );
            }
        }
    }
    let total_count = loaded.len();
    let disk_count = total_count - native_count;
    let leaked: &'static [LoadedPackage] = Box::leak(loaded.into_boxed_slice());
    *REGISTRY.write().expect("REGISTRY poisoned") = leaked;
    REGISTRY_INITIALIZED.store(true, Ordering::SeqCst);
    ReloadStats {
        native_count,
        disk_count,
        total_count,
    }
}

/// Every plugin currently registered (natives + disk packages). Empty
/// until [`init`] / [`init_many`] / [`reload`] runs.
pub fn packages() -> &'static [LoadedPackage] {
    *REGISTRY.read().expect("REGISTRY poisoned")
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

/// True if `model_id` resolves to a buildable processor, mirroring the
/// native-then-catalog resolution order every `build_*_processor_for_layout`
/// uses: a native model of its family that is available on this platform,
/// or a disk-package (NAM/IR/LV2/VST3) present in the catalog.
///
/// Issue #606: the per-family checks used to treat *any* non-native id as
/// available, so an uninstalled disk package slipped through to the native
/// registry and failed with a misleading "unsupported <family> model".
/// Routing the disk case through the catalog makes the block report
/// unavailable instead, so the caller can disable it and keep the chain
/// playing.
pub fn model_available(
    model_id: &str,
    is_native: impl Fn(&str) -> bool,
    native_available_on_platform: impl Fn(&str) -> bool,
) -> bool {
    if is_native(model_id) {
        native_available_on_platform(model_id)
    } else {
        find(model_id).is_some()
    }
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

/// Error returned by [`unload`] / [`load_one`] (#561 expanded scope).
#[derive(Debug, thiserror::Error)]
pub enum CatalogOpError {
    /// No plugin with that id is currently registered (for `unload`)
    /// or no disk package with that id exists under the scanned
    /// roots (for `load_one`).
    #[error("plugin not found: {0}")]
    NotFound(String),
    /// `unload` is forbidden for native plugins — the runtime fn
    /// pointers are compiled into the binary; dropping the manifest
    /// without re-registering would leave dangling references in
    /// every consumer that cached `&'static LoadedPackage`.
    #[error("plugin {0} is native (compiled-in); natives cannot be unloaded without restarting")]
    NativeCannotUnload(String),
}

/// Remove a disk plugin from the in-memory catalog by manifest id
/// (#561 expanded scope). Refuses natives — the runtime fn pointers
/// are part of the binary, and dropping the manifest while consumers
/// hold cached `&'static LoadedPackage` references would create a
/// gap between the public listing and the dispatch path.
///
/// Other invariants:
/// - Returns [`CatalogOpError::NotFound`] when no entry with `id`
///   exists in the current catalog.
/// - Atomically swaps in a new slice; cached references from before
///   the call stay valid (the previous slice is leaked, not freed),
///   just like [`reload`].
pub fn unload(id: &str) -> Result<(), CatalogOpError> {
    let current = packages();
    let Some(entry) = current.iter().find(|p| p.manifest.id == id) else {
        return Err(CatalogOpError::NotFound(id.to_string()));
    };
    if matches!(entry.manifest.backend, Backend::Native { .. }) {
        return Err(CatalogOpError::NativeCannotUnload(id.to_string()));
    }
    let next: Vec<LoadedPackage> = current
        .iter()
        .filter(|p| p.manifest.id != id)
        .map(|p| (*p).clone())
        .collect();
    let leaked: &'static [LoadedPackage] = Box::leak(next.into_boxed_slice());
    *REGISTRY.write().expect("REGISTRY poisoned") = leaked;
    Ok(())
}

/// Bring a single disk plugin into the in-memory catalog by manifest
/// id (#561 expanded scope). Scans every directory in `plugins_roots`,
/// adds the discovered package whose id matches, and atomically
/// swaps in the new slice. Other entries are preserved.
///
/// - If the plugin is already in the catalog, returns `Ok(())` as a
///   no-op (the dispatch surface is unchanged, but the caller still
///   gets a confirmation it had a chance to land).
/// - Returns [`CatalogOpError::NotFound`] when no package with `id`
///   is discoverable under any of the supplied roots.
pub fn load_one(id: &str, plugins_roots: &[std::path::PathBuf]) -> Result<(), CatalogOpError> {
    let current = packages();
    if current.iter().any(|p| p.manifest.id == id) {
        return Ok(());
    }
    let mut found: Option<LoadedPackage> = None;
    for root in plugins_roots {
        if !root.is_dir() {
            continue;
        }
        match discover(root) {
            Ok(results) => {
                for result in results.into_iter().flatten() {
                    if result.manifest.id == id {
                        found = Some(result);
                        break;
                    }
                }
            }
            Err(error) => {
                eprintln!(
                    "plugin-loader: cannot read plugins_root `{}`: {error}",
                    root.display()
                );
            }
        }
        if found.is_some() {
            break;
        }
    }
    let Some(new_entry) = found else {
        return Err(CatalogOpError::NotFound(id.to_string()));
    };
    let mut next: Vec<LoadedPackage> = current.iter().map(|p| (*p).clone()).collect();
    next.push(new_entry);
    let leaked: &'static [LoadedPackage] = Box::leak(next.into_boxed_slice());
    *REGISTRY.write().expect("REGISTRY poisoned") = leaked;
    Ok(())
}
