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
///
/// Opens the editor in a **separate floating window**.
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
    log::debug!("VST3 editor: no engine context for '{}', loading standalone instance", model_id);
    let uid = vst3_host::resolve_uid_for_model(model_id)?;
    let handle = vst3_host::open_vst3_editor_window_standalone(
        &entry.info.bundle_path,
        &uid,
        entry.display_name,
        sample_rate,
    )?;
    Ok(Box::new(handle))
}

/// Open the native editor in a child window attached to the given parent window.
///
/// The editor window has its own title bar (showing the plugin name) but is
/// registered as a child of `parent_ns_window` via `addChildWindow:ordered:`,
/// so it always floats above the parent and moves with it.
///
/// `parent_ns_window` is an `NSWindow*` (macOS) obtained from `raw-window-handle`.
///
/// Must be called on the main/UI thread.
pub fn open_vst3_editor_parented(
    model_id: &str,
    sample_rate: f64,
    parent_ns_window: *mut std::ffi::c_void,
) -> Result<Box<dyn PluginEditorHandle>> {
    let entry = vst3_host::find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    if let Some(gui_context) = vst3_host::lookup_vst3_gui_context(model_id) {
        log::debug!("VST3 editor (parented): reusing engine controller for '{}'", model_id);
        let handle = vst3_host::open_vst3_editor_window_parented(
            entry.display_name, gui_context, parent_ns_window,
        )?;
        return Ok(Box::new(handle));
    }

    // Fallback: standalone window, still parented.
    log::debug!("VST3 editor (parented): loading standalone instance for '{}'", model_id);
    let uid = vst3_host::resolve_uid_for_model(model_id)?;
    let handle = vst3_host::open_vst3_editor_window_standalone(
        &entry.info.bundle_path,
        &uid,
        entry.display_name,
        sample_rate,
    )?;
    // For standalone, we can't use parented variant easily since the plugin
    // is loaded fresh. Just use the regular standalone window for now.
    Ok(Box::new(handle))
}

/// Open the VST3 editor **embedded** inside a parent view.
///
/// `parent_view` is a platform-specific view handle (`NSView*` on macOS).
/// The plugin GUI is attached as a child view at `(x, y)` inside the parent,
/// using the parent's coordinate system (Slint-style top-left origin is
/// automatically converted if needed).
///
/// Returns `(handle, editor_width, editor_height)` so the caller can resize
/// the surrounding UI to fit the plugin GUI.
///
/// Must be called on the main/UI thread.
pub fn open_vst3_editor_embedded(
    model_id: &str,
    sample_rate: f64,
    parent_view: *mut std::ffi::c_void,
    x: f64,
    y: f64,
) -> Result<(Box<dyn PluginEditorHandle>, f64, f64)> {
    let entry = vst3_host::find_vst3_plugin(model_id)
        .ok_or_else(|| anyhow::anyhow!("VST3 plugin '{}' not found in catalog", model_id))?;

    if let Some(gui_context) = vst3_host::lookup_vst3_gui_context(model_id) {
        log::debug!("VST3 embedded editor: reusing engine controller for '{}'", model_id);
        let (handle, w, h) = vst3_host::open_vst3_editor_embedded(
            entry.display_name, gui_context, parent_view, x, y,
        )?;
        return Ok((Box::new(handle), w, h));
    }

    log::debug!("VST3 embedded editor: loading standalone instance for '{}'", model_id);
    let uid = vst3_host::resolve_uid_for_model(model_id)?;
    let (handle, w, h) = vst3_host::open_vst3_editor_embedded_standalone(
        &entry.info.bundle_path,
        &uid,
        entry.display_name,
        sample_rate,
        parent_view,
        x,
        y,
    )?;
    Ok((Box::new(handle), w, h))
}
