//! Responsibility: scans the system for VST3 bundles.
//! VST3 plugin discovery: scans system paths and individual bundles.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::host::{Vst3ParamInfo, Vst3Plugin, Vst3PluginClass};

// ---------------------------------------------------------------------------
// moduleinfo.json helpers (VST3 SDK 3.7+)
// ---------------------------------------------------------------------------

pub(crate) use crate::bundle_metadata::{read_info_plist_vendor, read_moduleinfo};
pub use crate::vst3_search_paths::system_vst3_paths;

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

/// Scan a single `.vst3` bundle directory — **safe mode**: zero dylib loading.
///
/// Strategy (in order):
/// 1. Try `Contents/Resources/moduleinfo.json` — present in VST3 SDK 3.7+ plugins.
///    Gives full class info (UID, name, vendor, category) with no `dlopen()`.
/// 2. Fall back to `Contents/Info.plist` for name/vendor only (UID unknown).
///    The plugin will be shown in the catalog but cannot be instantiated until
///    the user explicitly loads it.
///
/// Never calls `dlopen()` / `libloading::Library::new()`, so it is safe for
/// all plugins including those that deadlock or crash on load (e.g. Guitar Rig 7).
pub fn scan_vst3_bundle_light(bundle_path: &Path) -> Result<Vec<Vst3PluginInfo>> {
    // Strategy 1: moduleinfo.json (no dylib load, full UID).
    if let Some(infos) = read_moduleinfo(bundle_path) {
        log::debug!(
            "VST3 scan (moduleinfo): {} classes in {}",
            infos.len(),
            bundle_path.display()
        );
        return Ok(infos);
    }

    // Strategy 2: Info.plist — no UID, plugin name only.
    // We still add it to the catalog so the user can see it, but mark it as
    // "needs dylib load" by leaving uid = [0; 16].
    let name = read_info_plist_vendor(bundle_path);
    if name.is_empty() {
        anyhow::bail!(
            "no moduleinfo.json and no CFBundleName in {}",
            bundle_path.display()
        );
    }
    log::debug!(
        "VST3 scan (Info.plist fallback): '{}' in {}",
        name,
        bundle_path.display()
    );
    Ok(vec![Vst3PluginInfo {
        uid: [0u8; 16], // unknown until user loads it
        name,
        vendor: String::new(),
        category: "Audio Module Class".to_string(),
        bundle_path: bundle_path.to_path_buf(),
        params: Vec::new(),
        num_audio_inputs: 2,
        num_audio_outputs: 2,
    }])
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
    scan_vst3_dirs(&system_vst3_paths())
}

/// Scan the given directories for `.vst3` bundles (light mode), recursing into
/// sub-directories. Non-existent dirs are skipped silently.
///
/// Issue #776: lets discovery fold extra roots — the OpenRig plugins folder,
/// where catalog VST3 packages live at `<plugins_root>/vst3/<id>/bundles/` —
/// in alongside the standard system paths, so a catalog VST3 surfaces through
/// the exact same catalog / block-kind / editor path as a system-installed one.
pub fn scan_vst3_dirs(dirs: &[PathBuf]) -> Vec<Vst3PluginInfo> {
    let mut results = Vec::new();
    for root in dirs {
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
