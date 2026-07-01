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
/// Some VST3 plugins (ValhallaSupermassive, …) leave their module in a broken
/// state after an editor window is attached and then closed: the *next*
/// `IPluginFactory::createInstance` fails with `result=-1` for the rest of the
/// process, and releasing the old instance + reloading does NOT recover it.
///
/// So this registry keeps **at most one** editor per model alive for the whole
/// session and never rebuilds it. Re-opening reuses (re-focuses) the existing
/// window instead of creating a second instance.
#[derive(Default)]
pub struct Vst3EditorRegistry {
    open: HashMap<String, Box<dyn PluginEditorHandle>>,
}

impl Vst3EditorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the editor for `model_id`, or re-focus the one already open for it.
    ///
    /// If an editor for this model is already held, `open_fn` is NOT called —
    /// the existing window is brought to the front. This avoids the second
    /// `createInstance` that breaks plugins after a window close + reload.
    pub fn open_or_focus(
        &mut self,
        model_id: &str,
        open_fn: impl FnOnce() -> Result<Box<dyn PluginEditorHandle>>,
    ) -> Result<()> {
        if let Some(existing) = self.open.get(model_id) {
            existing.focus();
            return Ok(());
        }
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

    /// A fake editor handle that counts how many times it is re-focused.
    struct FakeHandle {
        focuses: Arc<AtomicUsize>,
    }
    impl PluginEditorHandle for FakeHandle {
        fn focus(&self) {
            self.focuses.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn reopening_same_model_reuses_and_focuses_the_open_editor() {
        let opens = Arc::new(AtomicUsize::new(0));
        let focuses = Arc::new(AtomicUsize::new(0));
        let mut reg = Vst3EditorRegistry::new();

        let opener = |opens: Arc<AtomicUsize>, focuses: Arc<AtomicUsize>| {
            move || -> Result<Box<dyn PluginEditorHandle>> {
                opens.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(FakeHandle { focuses }) as Box<dyn PluginEditorHandle>)
            }
        };

        reg.open_or_focus("valhalla", opener(opens.clone(), focuses.clone()))
            .expect("first open");
        reg.open_or_focus("valhalla", opener(opens.clone(), focuses.clone()))
            .expect("second open");

        // The second open must NOT build a new instance — it re-focuses the
        // existing one (a new createInstance would fail with result=-1).
        assert_eq!(opens.load(Ordering::SeqCst), 1, "instance created exactly once");
        assert_eq!(focuses.load(Ordering::SeqCst), 1, "existing editor re-focused");
    }
}
