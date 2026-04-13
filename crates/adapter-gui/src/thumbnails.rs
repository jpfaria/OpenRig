use std::path::PathBuf;
use std::sync::OnceLock;

/// Resolve the absolute path for a thumbnail image.
///
/// Searches relative to the executable first, then falls back to the
/// configured path directly — same strategy used by `ir::resolve_ir_capture`.
fn resolve_thumbnail_path(effect_type: &str, model_id: &str) -> Option<PathBuf> {
    let paths = infra_filesystem::asset_paths();
    let relative = format!("{}/{}.png", effect_type, model_id);

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.thumbnails).join(&relative)),
        Some(PathBuf::from(&paths.thumbnails).join(&relative)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    None
}

/// Returns the raw PNG bytes for a specific model thumbnail.
/// Fallback chain: exact (effect_type, model_id) -> (effect_type, "_default") -> None
///
/// Results are cached in a global cache so each file is read at most once.
pub fn thumbnail_png(effect_type: &str, model_id: &str) -> Option<Vec<u8>> {
    // Try exact match
    if let Some(bytes) = read_cached(effect_type, model_id) {
        return Some(bytes);
    }
    // Fallback to _default
    read_cached(effect_type, "_default")
}

fn read_cached(effect_type: &str, model_id: &str) -> Option<Vec<u8>> {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static CACHE: OnceLock<Mutex<HashMap<(String, String), Option<Vec<u8>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = (effect_type.to_string(), model_id.to_string());
    let mut map = cache.lock().ok()?;

    if let Some(entry) = map.get(&key) {
        return entry.clone();
    }

    let result = resolve_thumbnail_path(effect_type, model_id)
        .and_then(|path| std::fs::read(&path).ok());

    map.insert(key, result.clone());
    result
}

#[cfg(test)]
mod tests {
    // Note: resolve_thumbnail_path, thumbnail_png, and read_cached all depend
    // on infra_filesystem::asset_paths() being initialized, which requires
    // global state setup. These are integration-level functions rather than
    // pure functions, so we skip them here.
    //
    // The cache logic and path construction are implicitly tested when the
    // full GUI runs.

    #[test]
    fn module_compiles_and_exports_thumbnail_png() {
        // Verify the public API exists and has the expected signature
        let _: fn(&str, &str) -> Option<Vec<u8>> = super::thumbnail_png;
    }
}
