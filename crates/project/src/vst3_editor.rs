//! Facade for VST3 catalog initialisation (and #778 teardown marshalling).
//!
//! `adapter-gui` must not depend on `vst3-host` directly. The VST3 operations
//! the GUI layer needs are exposed here through `project`, the correct
//! dependency boundary for the adapter layer.
//!
//! #780: VST3 has no native plugin editor any more — a VST3 block is edited
//! through OpenRig's own knob editor like every other block (the knobs are
//! synthesised from the plugin's parameters, see `vst3_host::catalog_params`).

/// Initialise the VST3 plugin catalog by scanning standard system paths plus
/// the `vst3/` sub-directory of each configured plugin root (issue #776), so a
/// catalog VST3 shipped in the OpenRig plugins folder is discovered exactly
/// like a system-installed one.
///
/// Safe to call from a background thread. Subsequent calls are no-ops.
pub fn init_vst3_catalog(sample_rate: f64, plugin_roots: &[std::path::PathBuf]) {
    let extra_dirs: Vec<std::path::PathBuf> =
        plugin_roots.iter().map(|root| root.join("vst3")).collect();
    vst3_host::init_vst3_catalog(sample_rate, &extra_dirs);
}

/// Record the UI thread as the main/AppKit thread (issue #778). Call once on the
/// UI thread at startup so a VST3 plugin dropped on the control worker has its
/// teardown marshaled back here instead of crashing off the main thread.
pub fn mark_main_thread() {
    vst3_host::mark_main_thread();
}

/// Run any VST3 plugin teardown deferred from a non-main thread (issue #778).
/// Call on the frontend tick, on the main thread.
pub fn drain_deferred_vst3_teardowns() {
    vst3_host::drain_main_thread_deferred();
}
