use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

/// Metadata for a single plugin, loaded from a per-language YAML file.
#[derive(Deserialize, Clone, Default)]
pub struct PluginMetadata {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub homepage: String,
}

#[derive(Deserialize)]
struct MetadataFile {
    plugins: HashMap<String, PluginMetadata>,
}

/// Returns metadata for a plugin in the given language, or default (empty) if not found.
/// Results are cached — the YAML file is read at most once per language.
pub fn plugin_metadata(lang: &str, model_id: &str) -> PluginMetadata {
    static CACHE: OnceLock<Mutex<HashMap<String, HashMap<String, PluginMetadata>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let mut map = match cache.lock() {
        Ok(g) => g,
        Err(_) => return PluginMetadata::default(),
    };

    if !map.contains_key(lang) {
        let loaded = load_metadata_file(lang).unwrap_or_default();
        map.insert(lang.to_string(), loaded);
    }

    // Disk-package metadata wins over the legacy bulk YAML (issue
    // #287): each package can ship its own assets/metadata.yaml with
    // localized description / license / homepage. The migration
    // populated them from the bulk YAML keyed by legacy ids.
    if let Some(meta) = load_disk_package_metadata(lang, model_id) {
        return meta;
    }
    if let Some(meta) = map.get(lang).and_then(|m| m.get(model_id)).cloned() {
        return meta;
    }
    // Last-resort fallback: top-level manifest fields (single-language).
    if let Some(package) = plugin_loader::registry::find(model_id) {
        return PluginMetadata {
            description: package.manifest.description.clone().unwrap_or_default(),
            license: package.manifest.license.clone().unwrap_or_default(),
            homepage: package.manifest.homepage.clone().unwrap_or_default(),
        };
    }
    PluginMetadata::default()
}

#[derive(Deserialize)]
struct DiskPackageMetadataFile {
    #[serde(default)]
    i18n: HashMap<String, PluginMetadata>,
}

fn load_disk_package_metadata(lang: &str, model_id: &str) -> Option<PluginMetadata> {
    let package = plugin_loader::registry::find(model_id)?;
    let path = package.root.join("assets").join("metadata.yaml");
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let parsed: DiskPackageMetadataFile = serde_yaml::from_str(&content).ok()?;
    parsed
        .i18n
        .get(lang)
        .cloned()
        .or_else(|| parsed.i18n.get("en-US").cloned())
}

fn load_metadata_file(lang: &str) -> Option<HashMap<String, PluginMetadata>> {
    let paths = infra_filesystem::asset_paths();
    let file_name = format!("{}.yaml", lang);

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates: Vec<PathBuf> = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.metadata).join(&file_name)),
        Some(PathBuf::from(&paths.metadata).join(&file_name)),
    ]
    .into_iter()
    .flatten()
    .collect();

    for path in &candidates {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match serde_yaml::from_str::<MetadataFile>(&content) {
                    Ok(file) => return Some(file.plugins),
                    Err(e) => log::warn!("Failed to parse metadata {}: {}", path.display(), e),
                },
                Err(e) => log::warn!("Failed to read metadata {}: {}", path.display(), e),
            }
        }
    }
    None
}

/// Returns the raw PNG bytes for a plugin screenshot.
/// Tries: exact (effect_type, model_id) → (effect_type, "_default") → None
/// No global fallback — callers display the OpenRig logo placeholder when None.
pub fn screenshot_png(effect_type: &str, model_id: &str) -> Option<Vec<u8>> {
    read_screenshot_cached(effect_type, model_id)
        .or_else(|| read_screenshot_cached(effect_type, "_default"))
}

fn read_screenshot_cached(effect_type: &str, model_id: &str) -> Option<Vec<u8>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, String), Option<Vec<u8>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = (effect_type.to_string(), model_id.to_string());
    let mut map = match cache.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };

    if let Some(entry) = map.get(&key) {
        return entry.clone();
    }

    let result =
        resolve_screenshot_path(effect_type, model_id).and_then(|path| std::fs::read(&path).ok());

    map.insert(key, result.clone());
    result
}

fn resolve_screenshot_path(effect_type: &str, model_id: &str) -> Option<PathBuf> {
    let paths = infra_filesystem::asset_paths();
    let relative = if effect_type.is_empty() {
        format!("{}.png", model_id)
    } else {
        format!("{}/{}.png", effect_type, model_id)
    };

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.screenshots).join(&relative)),
        Some(PathBuf::from(&paths.screenshots).join(&relative)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    // Disk-package fallback (issue #287): plugin packages ship their
    // own screenshot under `<package_root>/assets/screenshot.png` (or
    // whatever path the manifest declares).
    if let Some(package) = plugin_loader::registry::find(model_id) {
        if let Some(rel) = package.manifest.screenshot.as_ref() {
            let candidate = package.root.join(rel);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Opens the given URL in the system's default browser.
pub fn open_homepage(url: &str) {
    if url.is_empty() {
        return;
    }
    if let Err(e) = webbrowser::open(url) {
        log::warn!("Failed to open URL {}: {}", url, e);
    }
}

#[cfg(test)]
#[path = "plugin_info_tests.rs"]
mod tests;
