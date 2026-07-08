//! Facade for VST3 catalog initialisation and plugin editor windows.
//!
//! `adapter-gui` must not depend on `vst3-host` directly. All VST3 operations
//! that the GUI layer needs are exposed here through `project`, which is the
//! correct dependency boundary for the adapter layer.

use anyhow::Result;
pub use block_core::PluginEditorHandle;
use std::collections::HashMap;

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
/// UI thread at startup so VST3 teardown that lands on the control worker is
/// marshaled back here instead of crashing off the main thread.
pub fn mark_main_thread() {
    vst3_host::mark_main_thread();
}

/// Run any VST3 plugin teardown deferred from a non-main thread (issue #778).
/// Call on the frontend tick, on the main thread.
pub fn drain_deferred_vst3_teardowns() {
    vst3_host::drain_main_thread_deferred();
}

/// The native editor may only open by reusing the engine's plugin instance.
///
/// `has_engine_context` is whether a `Vst3GuiContext` is registered for the
/// model (i.e. the engine has built it in an active chain). Loading a second,
/// standalone instance is intentionally NOT allowed: for plugins like
/// ValhallaSupermassive it corrupts the plugin module and breaks the engine's
/// audio instance (#251).
/// Whether the engine has already built this block's plugin (a `Vst3GuiContext`
/// is registered under its instance key), i.e. its editor can be opened by
/// reusing that instance. Keyed per block, not per model (#780).
pub fn has_engine_context(instance_key: &str) -> bool {
    vst3_host::lookup_vst3_gui_context(instance_key).is_some()
}

/// The plugin `model_id` of the block instance registered under `instance_key`,
/// or `None` if no context is registered. The open path uses this to resolve
/// the catalog entry (display name) from the block's own live instance instead
/// of being handed the model id (#780).
pub fn editor_model_for(instance_key: &str) -> Option<String> {
    vst3_host::lookup_vst3_gui_context(instance_key).map(|ctx| ctx.model_id)
}

pub fn require_engine_context(has_engine_context: bool) -> Result<()> {
    if !has_engine_context {
        anyhow::bail!(
            "plugin is not running yet — enable the chain and start audio, \
             then open the editor to tweak it live"
        );
    }
    Ok(())
}

/// Open the native editor window for the VST3 block registered under
/// `instance_key` (its `BlockId`, #780).
///
/// The editor ALWAYS reuses the plugin instance the audio engine built (its
/// registered `Vst3GuiContext`). It never loads a second, standalone instance:
/// a standalone copy corrupts the plugin module for plugins like
/// ValhallaSupermassive and breaks the engine's audio instance (#251). If the
/// engine has not built the plugin yet, opening is refused with a clear reason.
///
/// The plugin model — needed only to resolve the catalog display name — is
/// recovered from the block's own context, so two blocks of the same plugin
/// address their own instance.
///
/// `sample_rate` is unused now that standalone loading is gone; kept for the
/// call-site signature (the engine owns the instance's sample rate).
///
/// Must be called on the main/UI thread (macOS AppKit requirement).
pub fn open_vst3_editor(
    instance_key: &str,
    _sample_rate: f64,
) -> Result<Box<dyn PluginEditorHandle>> {
    let gui_context = vst3_host::lookup_vst3_gui_context(instance_key);
    require_engine_context(gui_context.is_some())?;
    let gui_context = gui_context.expect("engine context present after require_engine_context");

    let entry = vst3_host::find_vst3_plugin(&gui_context.model_id).ok_or_else(|| {
        anyhow::anyhow!(
            "VST3 plugin '{}' not found in catalog",
            gui_context.model_id
        )
    })?;

    log::info!(
        "VST3 editor: reusing engine controller for instance '{}' (model '{}')",
        instance_key,
        gui_context.model_id
    );
    let handle = vst3_host::open_vst3_editor_window(entry.display_name, gui_context)?;
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
        assert_eq!(
            opens.load(Ordering::SeqCst),
            1,
            "instance created exactly once"
        );
        assert_eq!(
            focuses.load(Ordering::SeqCst),
            1,
            "existing editor re-focused"
        );
    }
}
