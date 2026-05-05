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
/// Strategy:
/// 1. If the audio engine has already built this plugin (registered a
///    `Vst3GuiContext`), reuse its `IEditController`. This avoids creating a
///    second plugin instance, which fails for plugins like ValhallaSupermassive.
/// 2. Otherwise fall back to loading a fresh instance. This works for plugins
///    that allow multiple instances (Cloud Seed, Cocoa Delay, …) and lets the
///    user open the GUI before the engine has started.
///
/// Must be called on the main/UI thread (macOS AppKit requirement).
pub fn open_vst3_editor(model_id: &str, sample_rate: f64) -> Result<Box<dyn PluginEditorHandle>> {
    let entry = vst3_host::find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    if let Some(gui_context) = vst3_host::lookup_vst3_gui_context(model_id) {
        // Engine already loaded this plugin — reuse the existing controller.
        log::debug!("VST3 editor: reusing engine controller for '{}'", model_id);
        let handle = vst3_host::open_vst3_editor_window(entry.display_name, gui_context)?;
        return Ok(Box::new(handle));
    }

    // Fallback: load a standalone instance (no param-channel communication).
    log::debug!(
        "VST3 editor: no engine context for '{}', loading standalone instance",
        model_id
    );
    let uid = vst3_host::resolve_uid_for_model(model_id)?;
    let handle = vst3_host::open_vst3_editor_window_standalone(
        &entry.info.bundle_path,
        &uid,
        entry.display_name,
        sample_rate,
    )?;
    Ok(Box::new(handle))
}
