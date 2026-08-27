//! Responsibility: reads the parameters a catalogued VST3 plugin exposes.

use crate::host::Vst3Plugin;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::catalog::find_vst3_plugin;
use crate::catalog::PARAM_CACHE;

/// The VST3 model's parameters, for synthesising OpenRig knobs (#780).
///
/// The light discovery scan never loads a dylib, so `entry.info.params` is
/// empty. This fills that gap lazily and caches per model: it reads the metadata
/// from a LIVE instance's controller when one is registered (no extra load), and
/// otherwise loads a single throw-away instance to read it — safe only because
/// no instance of this model is streaming (else `live_params_for_model` returns
/// first, avoiding the #779 concurrent-instantiate crash). An empty result is
/// NOT cached, so knobs appear once the plugin has been loaded at least once.
pub fn catalog_params(model_id: &str) -> Vec<crate::host::Vst3ParamInfo> {
    let cache = PARAM_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .expect("vst3 param cache poisoned")
        .get(model_id)
    {
        return cached.clone();
    }
    let params = resolve_params(
        crate::param_registry::live_params_for_model(model_id),
        || load_and_read_params(model_id),
    );
    if !params.is_empty() {
        cache
            .lock()
            .expect("vst3 param cache poisoned")
            .insert(model_id.to_string(), params.clone());
    }
    params
}

/// Pick the parameter list: prefer a NON-EMPTY live read, otherwise fall back to
/// the loader. A live instance whose controller reads EMPTY (registered but not
/// yet reporting params) must NOT shadow the throw-away load — otherwise the
/// compact view, built while that live read is empty, shows a VST3 block with no
/// knobs while the detached editor (opened later) has them (#780).
pub(crate) fn resolve_params<T, F: FnOnce() -> Vec<T>>(live: Option<Vec<T>>, load: F) -> Vec<T> {
    match live {
        Some(params) if !params.is_empty() => params,
        _ => load(),
    }
}

/// Load a throw-away instance of `model_id` and read its parameter metadata.
/// Returns empty on any failure (missing entry, uid, or load error).
fn load_and_read_params(model_id: &str) -> Vec<crate::host::Vst3ParamInfo> {
    let Some(entry) = find_vst3_plugin(model_id) else {
        return Vec::new();
    };
    let Ok(uid) = crate::resolve_uid_for_model(model_id) else {
        return Vec::new();
    };
    match Vst3Plugin::load(&entry.info.bundle_path, &uid, 48_000.0, 2, 512, &[]) {
        Ok(plugin) => crate::param_registry::read_controller_params(plugin.controller()),
        Err(e) => {
            log::warn!("VST3 catalog_params: load '{}' failed: {}", model_id, e);
            Vec::new()
        }
    }
}
