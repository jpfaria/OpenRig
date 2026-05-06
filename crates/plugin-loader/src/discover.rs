//! Walks a bundle directory and loads every package found.
//!
//! Given a root like `plugins/bundle/`, this module produces one entry per
//! sub-folder that contains a `manifest.yaml`. Loading is best-effort: a
//! broken package surfaces as an error in its slot of the result vector
//! while the rest still load, so the caller can present a full report
//! instead of stopping at the first bad package.
//!
//! Issue: #287

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::manifest::{Backend, PluginManifest};
use crate::native_runtimes;
use crate::package::{validate_package, PackageError};

/// A package that loaded and validated successfully.
#[derive(Debug, Clone)]
pub struct LoadedPackage {
    /// Absolute (or caller-relative) path to the package directory.
    /// Empty for native plugins (they have no on-disk root).
    pub root: PathBuf,
    /// Parsed manifest.
    pub manifest: PluginManifest,
}

impl LoadedPackage {
    /// Instantiate the plugin into a [`BlockProcessor`].
    ///
    /// Native plugins dispatch via [`crate::native_runtimes`] (keyed by
    /// `runtime_id`). Disk-backed backends (NAM / IR / LV2 / VST3) go
    /// through the [`crate::package_builders`] table, which each
    /// backend crate populates at startup. The caller doesn't have to
    /// know which backend it's dealing with.
    pub fn build_processor(
        &self,
        params: &ParameterSet,
        sample_rate: f32,
        layout: AudioChannelLayout,
    ) -> Result<BlockProcessor> {
        if let Backend::Native { runtime_id } = &self.manifest.backend {
            let runtime = native_runtimes::get(runtime_id).ok_or_else(|| {
                anyhow!(
                    "no native runtime registered for `{runtime_id}` (plugin id `{}`)",
                    self.manifest.id
                )
            })?;
            return (runtime.build)(params, sample_rate, layout);
        }
        let kind = crate::package_builders::BackendKind::from_backend(&self.manifest.backend)
            .expect("Native handled above");
        let builder = crate::package_builders::get(kind).ok_or_else(|| {
            anyhow!(
                "no package builder registered for {:?} backend (plugin id `{}`)",
                kind,
                self.manifest.id
            )
        })?;
        builder(self, params, sample_rate, layout)
    }
}

/// One reason a single package failed to load.
///
/// `root` is always the package directory the failure refers to, so a
/// caller can show "package X failed: ..." messages without bookkeeping.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("package `{root}`: failed to read manifest.yaml: {source}")]
    ManifestFile {
        root: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("package `{root}`: invalid manifest.yaml: {source}")]
    ManifestParse {
        root: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("package `{root}`: validation failed: {source}")]
    Package {
        root: PathBuf,
        #[source]
        source: PackageError,
    },
}

/// Discover every package under `bundle_root`.
///
/// Walks the directory tree recursively. A directory containing a
/// `manifest.yaml` is treated as a package and not descended into; the
/// repo layout groups packages by backend (`<root>/<backend>/<id>/...`),
/// so the walker has to look more than one level deep, and packages
/// themselves often contain asset sub-trees that must not be re-scanned.
///
/// Per-package failures are collected, not propagated, so a single broken
/// package doesn't hide the rest of the catalog. The outer `io::Result`
/// fails only when the bundle root itself can't be read.
pub fn discover(bundle_root: &Path) -> io::Result<Vec<Result<LoadedPackage, DiscoveryError>>> {
    let mut results = Vec::new();
    walk(bundle_root, &mut results)?;
    results.sort_by(|left, right| {
        let left_root: &Path = match left {
            Ok(loaded) => &loaded.root,
            Err(error) => discovery_error_root(error),
        };
        let right_root: &Path = match right {
            Ok(loaded) => &loaded.root,
            Err(error) => discovery_error_root(error),
        };
        left_root.cmp(right_root)
    });
    Ok(results)
}

/// Recurse into `dir`. If `dir/manifest.yaml` exists, treat `dir` as a
/// package (load it, don't descend). Otherwise list entries and recurse
/// into each sub-directory. Loose files at any level are skipped.
fn walk(dir: &Path, results: &mut Vec<Result<LoadedPackage, DiscoveryError>>) -> io::Result<()> {
    let manifest_path = dir.join("manifest.yaml");
    if manifest_path.is_file() {
        results.push(load_package(dir, &manifest_path));
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        walk(&path, results)?;
    }
    Ok(())
}

fn discovery_error_root(error: &DiscoveryError) -> &Path {
    match error {
        DiscoveryError::ManifestFile { root, .. }
        | DiscoveryError::ManifestParse { root, .. }
        | DiscoveryError::Package { root, .. } => root,
    }
}

fn load_package(root: &Path, manifest_path: &Path) -> Result<LoadedPackage, DiscoveryError> {
    let yaml =
        fs::read_to_string(manifest_path).map_err(|source| DiscoveryError::ManifestFile {
            root: root.to_path_buf(),
            source,
        })?;
    let manifest: PluginManifest =
        serde_yaml::from_str(&yaml).map_err(|source| DiscoveryError::ManifestParse {
            root: root.to_path_buf(),
            source,
        })?;
    validate_package(root, &manifest).map_err(|source| DiscoveryError::Package {
        root: root.to_path_buf(),
        source,
    })?;
    Ok(LoadedPackage {
        root: root.to_path_buf(),
        manifest,
    })
}

#[cfg(test)]
#[path = "discover_tests.rs"]
mod tests;
