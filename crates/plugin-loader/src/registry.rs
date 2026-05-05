//! Process-wide registry of discovered plugin packages.
//!
//! `init(plugins_root)` runs `discover()` once at app startup and caches
//! the result. After that every block-* crate (and the GUI) reaches the
//! loaded catalog via [`packages`], [`packages_for`], or [`find`] without
//! threading the path through every call site.
//!
//! Issue: #287

use std::path::Path;
use std::sync::OnceLock;

use crate::discover::{discover, LoadedPackage};
use crate::manifest::BlockType;

static REGISTRY: OnceLock<Vec<LoadedPackage>> = OnceLock::new();

/// Discover every package under `plugins_root` and store the result.
///
/// Idempotent: the second call is a no-op (the catalog never changes
/// during a process lifetime). Per-package failures are dropped — the
/// engine boot path can't surface them per-block, but the loader logs
/// each broken package's error for diagnostic visibility.
pub fn init(plugins_root: &Path) {
    if REGISTRY.get().is_some() {
        return;
    }
    let mut loaded: Vec<LoadedPackage> = Vec::new();
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

/// Every package currently loaded. Empty until [`init`] runs.
pub fn packages() -> &'static [LoadedPackage] {
    match REGISTRY.get() {
        Some(packages) => packages,
        None => &[],
    }
}

/// Packages whose manifest declares `block_type`. Returned in the same
/// order discovery produced (alphabetical by package directory).
pub fn packages_for(block_type: BlockType) -> Vec<&'static LoadedPackage> {
    packages()
        .iter()
        .filter(|p| p.manifest.block_type == block_type)
        .collect()
}

/// Look up a single package by manifest id (`p.manifest.id`).
pub fn find(model_id: &str) -> Option<&'static LoadedPackage> {
    packages().iter().find(|p| p.manifest.id == model_id)
}
