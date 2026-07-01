//! Facade for VST3 catalog initialisation and plugin editor windows.
//!
//! `adapter-gui` must not depend on `vst3-host` directly. All VST3 operations
//! that the GUI layer needs are exposed here through `project`, which is the
//! correct dependency boundary for the adapter layer.

use anyhow::Result;
use std::collections::HashMap;
pub use block_core::PluginEditorHandle;

/// Tracks the open native editor windows, keyed by `model_id`.
///
/// Single-instance VST3 plugins (ValhallaSupermassive, …) reject a second
/// `IPluginFactory::createInstance` while the first instance is still alive.
/// The GUI used to `push` every editor handle into a `Vec` and never drop it,
/// so re-opening such a plugin's editor failed with `createInstance result=-1`.
///
/// This registry keeps **at most one** open editor per model and drops the
/// previous handle (closing its window and releasing the plugin instance)
/// *before* the new instance is created, so re-opening always works.
#[derive(Default)]
pub struct Vst3EditorRegistry {
    open: HashMap<String, Box<dyn PluginEditorHandle>>,
}

impl Vst3EditorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open (or re-open) the editor for `model_id`, replacing any editor
    /// already open for the same model. `open_fn` constructs the fresh handle.
    pub fn open_or_replace(
        &mut self,
        model_id: &str,
        open_fn: impl FnOnce() -> Result<Box<dyn PluginEditorHandle>>,
    ) -> Result<()> {
        // Drop the previous editor for this model FIRST, releasing its plugin
        // instance, so single-instance plugins can create a fresh instance.
        drop(self.open.remove(model_id));
        let handle = open_fn()?;
        self.open.insert(model_id.to_string(), handle);
        Ok(())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A fake editor handle that decrements a shared "live instances" counter
    /// when dropped, so a test can observe when the plugin instance is released.
    struct FakeHandle {
        live: Arc<AtomicUsize>,
    }
    impl PluginEditorHandle for FakeHandle {}
    impl Drop for FakeHandle {
        fn drop(&mut self) {
            self.live.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Builds an `open_fn` for a **single-instance** plugin: constructing a new
    /// instance while one is already alive fails, mirroring ValhallaSupermassive
    /// returning `createInstance result=-1`.
    fn single_instance_opener(
        live: Arc<AtomicUsize>,
    ) -> impl FnOnce() -> Result<Box<dyn PluginEditorHandle>> {
        move || {
            if live.load(Ordering::SeqCst) != 0 {
                anyhow::bail!("single-instance plugin: an instance is already alive");
            }
            live.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeHandle { live }) as Box<dyn PluginEditorHandle>)
        }
    }

    #[test]
    fn reopening_single_instance_plugin_drops_previous_instance_first() {
        let live = Arc::new(AtomicUsize::new(0));
        let mut reg = Vst3EditorRegistry::new();

        reg.open_or_replace("valhalla", single_instance_opener(live.clone()))
            .expect("first open should succeed");
        assert_eq!(live.load(Ordering::SeqCst), 1, "one instance alive");

        // Re-opening must succeed: the previous instance is released BEFORE the
        // new one is created, otherwise createInstance would fail (result=-1).
        reg.open_or_replace("valhalla", single_instance_opener(live.clone()))
            .expect("re-open must succeed after releasing the previous instance");
        assert_eq!(live.load(Ordering::SeqCst), 1, "still exactly one instance");
    }
}
