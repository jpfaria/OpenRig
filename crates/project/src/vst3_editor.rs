//! Facade for VST3 catalog initialisation and plugin editor windows.
//!
//! `adapter-gui` must not depend on `vst3-host` directly. All VST3 operations
//! that the GUI layer needs are exposed here through `project`, which is the
//! correct dependency boundary for the adapter layer.

use anyhow::Result;
pub use block_core::PluginEditorHandle;

/// Initialise the VST3 plugin catalog by scanning standard system paths.
///
/// Safe to call from a background thread. Subsequent calls are no-ops.
pub fn init_vst3_catalog(sample_rate: f64) {
    vst3_host::init_vst3_catalog(sample_rate);
}

/// Open the native editor window for the VST3 plugin identified by `model_id`.
///
/// Loads a separate plugin instance dedicated to the GUI. Resolves the UID
/// lazily if the plugin lacks `moduleinfo.json`. Returns a handle that keeps
/// the window alive; drop it to close.
///
/// Must be called on the main/UI thread (macOS AppKit requirement).
pub fn open_vst3_editor(model_id: &str, sample_rate: f64) -> Result<Box<dyn PluginEditorHandle>> {
    let uid = vst3_host::resolve_uid_for_model(model_id)?;
    let entry = vst3_host::find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    let handle = vst3_host::open_vst3_editor_window(
        &entry.info.bundle_path,
        &uid,
        entry.display_name,
        sample_rate,
    )?;

    Ok(Box::new(handle))
}
