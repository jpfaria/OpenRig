//! Responsibility: remembers the class uid a bundle reported for a model.

use crate::catalog::{find_vst3_plugin, UID_CACHE};
use crate::discovery::Vst3PluginInfo;
use crate::host::Vst3Plugin;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) fn uid_cache() -> &'static Mutex<HashMap<PathBuf, HashMap<String, [u8; 16]>>> {
    UID_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Leak a `String` into a `&'static str`.
///
/// Safe because the catalog is initialised once and never dropped.
pub(crate) fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Build the stable model ID for a discovered plugin.
pub fn make_model_id(info: &Vst3PluginInfo) -> String {
    let bundle_stem = info
        .bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let class_name = info.name.replace(' ', "_");
    format!("vst3:{}:{}", bundle_stem, class_name)
}

/// Resolve the UID for a catalog entry.
///
/// If the UID was already known from `moduleinfo.json` (uid != [0;16]), returns it
/// immediately. Otherwise, performs a lazy `enumerate_classes()` call to discover
/// the UID from the plugin factory, caches the result, and returns it.
///
/// **Warning**: For plugins without `moduleinfo.json` (e.g. ValhallaSupermassive,
/// Guitar Rig 7) this will call `dlopen()` on the plugin dylib. Most plugins are
/// safe, but some complex commercial plugins may deadlock or crash the process.
pub fn resolve_uid_for_model(model_id: &str) -> anyhow::Result<[u8; 16]> {
    let entry = find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    // Fast path: UID already known from moduleinfo.json.
    if entry.info.uid != [0u8; 16] {
        return Ok(entry.info.uid);
    }

    let bundle_path = &entry.info.bundle_path;
    let class_name = entry.display_name;

    // Check cache first.
    {
        let cache = uid_cache().lock().unwrap();
        if let Some(by_class) = cache.get(bundle_path) {
            if let Some(&uid) = by_class.get(class_name) {
                return Ok(uid);
            }
        }
    }

    // Lazy resolution via enumerate_classes (performs dlopen).
    log::info!(
        "VST3: lazy UID resolution for '{}' in {}",
        class_name,
        bundle_path.display()
    );
    let (_lib, classes) = Vst3Plugin::enumerate_classes(bundle_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to enumerate classes in {}: {}",
            bundle_path.display(),
            e
        )
    })?;

    // Pick the AUDIO MODULE CLASS (the IComponent audio processor) for a given
    // class name. A plugin can expose several factory classes sharing the SAME
    // name — e.g. ValhallaSupermassive ships both an "Audio Module Class" and a
    // "Component Controller Class". Only the Audio Module Class implements
    // IComponent; instantiating any other with IComponent returns kNoInterface
    // (-1) and the block faults into bypass (#251). Fall back to a name-only
    // match if the plugin doesn't tag categories.
    let pick_uid = |name: &str| -> Option<[u8; 16]> {
        classes
            .iter()
            .find(|c| c.name == name && c.category.contains("Audio Module Class"))
            .or_else(|| classes.iter().find(|c| c.name == name))
            .map(|c| c.uid)
    };

    // Cache the Audio-Module uid per distinct class name, so a same-named
    // controller (enumerated later) can never overwrite the processor's uid —
    // that overwrite is what made the 2nd+ resolve return the wrong class.
    let mut cache = uid_cache().lock().unwrap();
    let by_class = cache.entry(bundle_path.clone()).or_default();
    for cls in &classes {
        if let Some(uid) = pick_uid(&cls.name) {
            by_class.insert(cls.name.clone(), uid);
        }
    }

    pick_uid(class_name).ok_or_else(|| {
        anyhow::anyhow!(
            "class '{}' not found in bundle {} (found: {})",
            class_name,
            bundle_path.display(),
            classes
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}
