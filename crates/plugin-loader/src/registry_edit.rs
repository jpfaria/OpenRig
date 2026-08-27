//! Responsibility: loads or unloads one package while the app runs.

use crate::discover::{discover, LoadedPackage};
use crate::manifest::Backend;
use crate::registry::REGISTRY;
use crate::registry_query::packages;

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
