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
/// Reuses the `IEditController` from the audio processor (via the registered
/// `Vst3GuiContext`) instead of loading a second plugin instance. This avoids
/// failures with plugins like ValhallaSupermassive that reject multiple instances.
///
/// Returns an error if the plugin has not been loaded by the audio engine yet
/// (i.e. there is no registered GUI context for `model_id`).
///
/// Must be called on the main/UI thread (macOS AppKit requirement).
pub fn open_vst3_editor(model_id: &str, _sample_rate: f64) -> Result<Box<dyn PluginEditorHandle>> {
    let entry = vst3_host::find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    let gui_context = vst3_host::lookup_vst3_gui_context(model_id)
        .ok_or_else(|| anyhow::anyhow!(
            "VST3 plugin '{}' is not loaded in the audio engine yet — \
             add it to a chain first",
            model_id
        ))?;

    let handle = vst3_host::open_vst3_editor_window(entry.display_name, gui_context)?;
    Ok(Box::new(handle))
}
