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

/// Scan a single `.vst3` bundle directory — **light mode**: only reads factory
/// class info without fully instantiating any plugin.
///
/// This is safe for all plugins (including complex commercial ones that may crash
/// on full initialisation). The returned `Vst3PluginInfo` entries will have
/// `params` empty and `num_audio_inputs/outputs` defaulted to 2.
///
/// Returns an error only if the factory cannot be opened at all.
pub fn scan_vst3_bundle_light(bundle_path: &Path) -> Result<Vec<Vst3PluginInfo>> {
    let vendor = Vst3Plugin::factory_vendor(bundle_path);
    let (_lib, classes) = Vst3Plugin::enumerate_classes(bundle_path)?;

    let results = classes
        .into_iter()
        .filter(|c| c.category.contains("Audio Module Class") || c.category.contains("Audio"))
        .map(|class| Vst3PluginInfo {
            uid: class.uid,
            name: class.name,
            vendor: vendor.clone(),
            category: class.category,
            bundle_path: bundle_path.to_path_buf(),
            params: Vec::new(),
            num_audio_inputs: 2,
            num_audio_outputs: 2,
        })
        .collect();

    Ok(results)
}

/// Scan a single `.vst3` bundle directory — **full mode**: fully instantiates
/// each plugin class to enumerate its parameters and bus layout.
///
/// `sample_rate` is needed to initialise the plugin for parameter enumeration.
/// Returns an error only if the bundle cannot be loaded at all; individual class
/// failures are logged and skipped.
///
/// **Warning**: some complex commercial plugins (e.g. Guitar Rig, Kontakt) may
/// crash the process during full initialisation. Prefer `scan_vst3_bundle_light`
/// for system-wide discovery and reserve this for known-safe plugins only.
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

/// Resolve the full path to a `.vst3` bundle by its directory name.
///
/// Searches the standard system VST3 paths (user-level first, then system) for
/// a bundle whose directory name equals `bundle_name` (e.g. `"CloudSeed.vst3"`).
///
/// Returns an error if the bundle is not found in any search path.
pub fn resolve_vst3_bundle(bundle_name: &str) -> Result<PathBuf> {
    for root in system_vst3_paths() {
        let candidate = root.join(bundle_name);
        if candidate.exists() {
            return Ok(candidate);
        }
        // Also search one level deep (some installers create a sub-directory).
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let sub = entry.path().join(bundle_name);
                if sub.exists() {
                    return Ok(sub);
                }
            }
        }
    }
    anyhow::bail!(
        "VST3 bundle '{}' not found in system VST3 paths: {:?}",
        bundle_name,
        system_vst3_paths()
    )
}

/// Scan all standard system VST3 paths and return discovered plugins (light mode).
///
/// Uses `scan_vst3_bundle_light` — only reads factory class info, never fully
/// instantiates plugins. This is safe for all plugins including complex commercial
/// ones that may crash on full initialisation.
///
/// Bundles that fail to open are silently skipped (errors are logged).
pub fn scan_system_vst3(_sample_rate: f64) -> Vec<Vst3PluginInfo> {
    let search_paths = system_vst3_paths();
    let mut results = Vec::new();

    for root in &search_paths {
        if !root.exists() {
            continue;
        }
        scan_directory_light(root, &mut results);
    }

    results
}

/// Recursively walk `dir` looking for `.vst3` bundle directories (light scan).
fn scan_directory_light(dir: &Path, results: &mut Vec<Vst3PluginInfo>) {
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
                match scan_vst3_bundle_light(&path) {
                    Ok(infos) => results.extend(infos),
                    Err(e) => {
                        log::debug!("VST3 scan: skipping {}: {}", path.display(), e);
                    }
                }
            } else {
                scan_directory_light(&path, results);
            }
        }
    }
}
