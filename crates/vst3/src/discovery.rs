//! VST3 plugin discovery: scans system paths and individual bundles.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::host::{Vst3ParamInfo, Vst3Plugin, Vst3PluginClass};

/// Information about a discovered VST3 plugin.
#[derive(Debug, Clone)]
pub struct Vst3PluginInfo {
    pub uid: [u8; 16],
    pub name: String,
    pub vendor: String,
    pub category: String,
    pub bundle_path: PathBuf,
    pub params: Vec<Vst3ParamInfo>,
    pub num_audio_inputs: i32,
    pub num_audio_outputs: i32,
}

/// Returns the standard system VST3 search paths for the current platform.
///
/// Paths are returned in priority order (user-level first, system-level second).
/// None of these paths are guaranteed to exist.
pub fn system_vst3_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut paths = Vec::new();
        // User-level
        if let Some(home) = dirs_home() {
            paths.push(home.join("Library").join("Audio").join("Plug-Ins").join("VST3"));
        }
        // System-level
        paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        // Network / developer
        paths.push(PathBuf::from("/Network/Library/Audio/Plug-Ins/VST3"));
        paths
    }
    #[cfg(target_os = "windows")]
    {
        let mut paths = Vec::new();
        // %LOCALAPPDATA%\Programs\Common\VST3
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("Common")
                    .join("VST3"),
            );
        }
        // %PROGRAMFILES%\Common Files\VST3
        if let Some(pf) = std::env::var_os("PROGRAMFILES") {
            paths.push(PathBuf::from(pf).join("Common Files").join("VST3"));
        }
        // %PROGRAMFILES(X86)%\Common Files\VST3
        if let Some(pf86) = std::env::var_os("PROGRAMFILES(X86)") {
            paths.push(PathBuf::from(pf86).join("Common Files").join("VST3"));
        }
        paths
    }
    #[cfg(target_os = "linux")]
    {
        let mut paths = Vec::new();
        // User-level
        if let Some(home) = dirs_home() {
            paths.push(home.join(".vst3"));
        }
        // System-level
        paths.push(PathBuf::from("/usr/lib/vst3"));
        paths.push(PathBuf::from("/usr/local/lib/vst3"));
        paths
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// Resolve the user home directory without depending on the `dirs` crate.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| {
            #[cfg(target_os = "windows")]
            {
                std::env::var_os("USERPROFILE").map(PathBuf::from)
            }
            #[cfg(not(target_os = "windows"))]
            {
                None
            }
        })
}

/// Scan a single `.vst3` bundle directory and return info for each plugin class.
///
/// `sample_rate` is needed to initialise the plugin for parameter enumeration.
/// Returns an error only if the bundle cannot be loaded at all; individual class
/// failures are logged and skipped.
pub fn scan_vst3_bundle(bundle_path: &Path, sample_rate: f64) -> Result<Vec<Vst3PluginInfo>> {
    let vendor = Vst3Plugin::factory_vendor(bundle_path);

    // Enumerate classes without fully initialising the plugin.
    let (_lib, classes) = Vst3Plugin::enumerate_classes(bundle_path)?;

    // Only process audio effect classes ("Audio Module Class" category).
    let fx_classes: Vec<Vst3PluginClass> = classes
        .into_iter()
        .filter(|c| c.category.contains("Audio Module Class") || c.category.contains("Audio"))
        .collect();

    drop(_lib); // Release factory before loading individual instances.

    let mut results = Vec::new();

    for class in fx_classes {
        // Fully load the plugin to read its parameters and bus info.
        let plugin = match Vst3Plugin::load(
            bundle_path,
            &class.uid,
            sample_rate,
            2, // stereo for discovery
            512,
            &[],
        ) {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "VST3 scan: failed to load class '{}' in {}: {}",
                    class.name,
                    bundle_path.display(),
                    e
                );
                continue;
            }
        };

        let param_count = plugin.param_count();
        let mut params = Vec::new();
        for i in 0..param_count {
            match plugin.param_info(i) {
                Ok(info) => params.push(info),
                Err(e) => {
                    log::trace!("VST3 scan: param_info({}) failed: {}", i, e);
                }
            }
        }

        results.push(Vst3PluginInfo {
            uid: class.uid,
            name: class.name,
            vendor: vendor.clone(),
            category: class.category,
            bundle_path: bundle_path.to_path_buf(),
            params,
            num_audio_inputs: plugin.num_input_channels,
            num_audio_outputs: plugin.num_output_channels,
        });
    }

    Ok(results)
}

/// Scan all standard system VST3 paths and return discovered plugins.
///
/// Bundles that fail to load are silently skipped (errors are logged).
pub fn scan_system_vst3(sample_rate: f64) -> Vec<Vst3PluginInfo> {
    let search_paths = system_vst3_paths();
    let mut results = Vec::new();

    for root in &search_paths {
        if !root.exists() {
            continue;
        }
        scan_directory(root, sample_rate, &mut results);
    }

    results
}

/// Recursively walk `dir` looking for `.vst3` bundle directories.
fn scan_directory(dir: &Path, sample_rate: f64, results: &mut Vec<Vst3PluginInfo>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("VST3 scan: cannot read dir {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.extension().and_then(|e| e.to_str()) == Some("vst3") {
                // This is a VST3 bundle directory.
                match scan_vst3_bundle(&path, sample_rate) {
                    Ok(infos) => results.extend(infos),
                    Err(e) => {
                        log::debug!(
                            "VST3 scan: skipping {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            } else {
                // Recurse into sub-directories (some installers nest bundles).
                scan_directory(&path, sample_rate, results);
            }
        }
    }
}
