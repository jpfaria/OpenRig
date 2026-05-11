//! VST3 plugin discovery: scans system paths and individual bundles.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::host::{Vst3ParamInfo, Vst3Plugin, Vst3PluginClass};

// ---------------------------------------------------------------------------
// moduleinfo.json helpers (VST3 SDK 3.7+)
// ---------------------------------------------------------------------------

/// Parse `Contents/Resources/moduleinfo.json` without loading the plugin dylib.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
fn read_moduleinfo(bundle_path: &Path) -> Option<Vec<Vst3PluginInfo>> {
    let json_path = bundle_path
        .join("Contents")
        .join("Resources")
        .join("moduleinfo.json");

    let raw = std::fs::read_to_string(&json_path).ok()?;

    // Parse vendor from Factory Info
    let vendor = extract_json_string(&raw, "Vendor").unwrap_or_default();

    // Find all "Audio Module Class" entries in the Classes array.
    let mut results = Vec::new();
    let mut pos = 0;
    while let Some(class_start) = raw[pos..].find("\"CID\"") {
        let base = pos + class_start;
        let chunk_end = raw[base..]
            .find('}')
            .map(|i| base + i + 1)
            .unwrap_or(raw.len());
        let chunk = &raw[base..chunk_end];

        let category = extract_json_string(chunk, "Category").unwrap_or_default();
        if !category.contains("Audio Module Class") {
            pos = chunk_end;
            continue;
        }

        let cid_hex = extract_json_string(chunk, "CID").unwrap_or_default();
        let uid = parse_cid_hex(&cid_hex);
        let name = extract_json_string(chunk, "Name").unwrap_or_else(|| "Unknown".to_string());

        if let Some(uid) = uid {
            results.push(Vst3PluginInfo {
                uid,
                name,
                vendor: vendor.clone(),
                category,
                bundle_path: bundle_path.to_path_buf(),
                params: Vec::new(),
                num_audio_inputs: 2,
                num_audio_outputs: 2,
            });
        }
        pos = chunk_end;
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Extract a JSON string value for a given key (simple, no full parser needed).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let start = json.find(&needle)?;
    let after_key = &json[start + needle.len()..];
    let colon = after_key.find(':')? + 1;
    let after_colon = after_key[colon..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let inner = &after_colon[1..];
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

/// Parse a 32-hex-char CID string (e.g. "ABCDEF019182FAEB476E617547637332") into
/// a 16-byte array.
fn parse_cid_hex(hex: &str) -> Option<[u8; 16]> {
    let hex = hex.trim();
    if hex.len() != 32 {
        return None;
    }
    let mut uid = [0u8; 16];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).ok()?;
        uid[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(uid)
}

/// Read vendor from `Contents/Info.plist` (fallback for bundles without moduleinfo.json).
fn read_info_plist_vendor(bundle_path: &Path) -> String {
    let plist_path = bundle_path.join("Contents").join("Info.plist");
    let raw = match std::fs::read_to_string(&plist_path) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    // Look for CFBundleName or NSHumanReadableCopyright as vendor hint.
    extract_plist_string(&raw, "CFBundleName").unwrap_or_default()
}

/// Extract a string value from an Apple plist (XML format) without a full parser.
fn extract_plist_string(plist: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{}</key>", key);
    let start = plist.find(&needle)? + needle.len();
    let after = &plist[start..];
    let str_start = after.find("<string>")? + "<string>".len();
    let str_end = after[str_start..].find("</string>")?;
    Some(after[str_start..str_start + str_end].to_string())
}

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
            paths.push(
                home.join("Library")
                    .join("Audio")
                    .join("Plug-Ins")
                    .join("VST3"),
            );
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

#[cfg(not(target_os = "windows"))]
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
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
