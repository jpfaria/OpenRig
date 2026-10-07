//! Responsibility: lists the folders the VST3 catalog scans for bundles.

use std::path::PathBuf;

use plugin_loader::manifest::Backend;
use plugin_loader::LoadedPackage;

/// The folders the VST3 catalog walks for `.vst3` bundles, besides the
/// system paths: the `vst3/` sub-directory of each plugin root (issue #776,
/// bundles dropped there without a manifest) and the folder of every VST3
/// package the plugin loader found, wherever it sits under its root — a dev
/// run scans `<cwd>/plugins`, whose packages live under `source/vst3/`
/// (#1093). A bundle reached twice is kept once by the catalog.
pub fn vst3_scan_dirs(plugin_roots: &[PathBuf], packages: &[LoadedPackage]) -> Vec<PathBuf> {
    let package_dirs = packages
        .iter()
        .filter(|package| matches!(package.manifest.backend, Backend::Vst3 { .. }))
        .map(|package| package.root.clone());
    plugin_roots
        .iter()
        .map(|root| root.join("vst3"))
        .chain(package_dirs)
        .collect()
}

#[cfg(test)]
#[path = "vst3_scan_dirs_tests.rs"]
mod tests;
