//! Responsibility: fills the catalog from the plugin roots on disk.

use crate::discover::{discover, LoadedPackage};
use crate::registry::{NATIVES, REGISTRY, REGISTRY_INITIALIZED};
use std::path::Path;
use std::sync::atomic::Ordering;

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
/// Used by `PluginCommand::ReloadPluginCatalog` (issue #561) so the running
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
