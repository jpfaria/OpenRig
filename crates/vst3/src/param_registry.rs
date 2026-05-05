//! Global registry that maps VST3 `model_id` → `Vst3GuiContext`.
//!
//! The engine registers a context (param channel + shared controller + library
//! Arc) when it builds a `Vst3Processor`. When the GUI opens the native editor
//! it looks up the same context so the editor reuses the existing controller
//! instead of creating a second plugin instance (which fails for plugins like
//! ValhallaSupermassive that reject multiple instances).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use vst3::ComPtr;
use vst3::Steinberg::Vst::IEditController;

use crate::param_channel::{vst3_param_channel, Vst3ParamChannel};

/// Everything the GUI needs to open and drive the native editor window without
/// creating a second plugin instance.
pub struct Vst3GuiContext {
    /// Lock-free queue shared between the GUI and the audio processor.
    pub param_channel: Vst3ParamChannel,
    /// Reference-counted pointer to the controller already held by the audio
    /// processor. The GUI uses it to call `createView` and register
    /// `IComponentHandler`.
    pub controller: ComPtr<IEditController>,
    /// Keeps the plugin dylib alive while the editor window is open, even if
    /// the audio processor is dropped first.
    pub library: Arc<libloading::Library>,
}

// SAFETY: `ComPtr<IEditController>` is a reference-counted COM pointer. The
// controller is only ever used on the main/UI thread from the editor side.
// `Arc<libloading::Library>` is inherently Send+Sync.
unsafe impl Send for Vst3GuiContext {}
unsafe impl Sync for Vst3GuiContext {}

static REGISTRY: OnceLock<RwLock<HashMap<String, Vst3GuiContext>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, Vst3GuiContext>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a GUI context for `model_id`, replacing any previous entry.
///
/// Called by the engine after loading the plugin. Returns the newly created
/// `Vst3ParamChannel` so the caller can attach it to its processor instance.
pub fn register_vst3_gui_context(
    model_id: &str,
    controller: ComPtr<IEditController>,
    library: Arc<libloading::Library>,
) -> Vst3ParamChannel {
    let channel = vst3_param_channel();
    registry()
        .write()
        .expect("vst3 param registry poisoned")
        .insert(
            model_id.to_string(),
            Vst3GuiContext {
                param_channel: channel.clone(),
                controller,
                library,
            },
        );
    log::debug!("VST3 registry: registered context for '{}'", model_id);
    channel
}

/// Look up the `Vst3GuiContext` previously registered for `model_id`.
///
/// Returns `None` if no processor for this model has been built yet.
/// The returned context clones the COM pointer and `Arc` (cheap ref-count bumps).
pub fn lookup_vst3_gui_context(model_id: &str) -> Option<Vst3GuiContext> {
    let guard = registry().read().expect("vst3 param registry poisoned");
    let ctx = guard.get(model_id)?;
    Some(Vst3GuiContext {
        param_channel: ctx.param_channel.clone(),
        controller: ctx.controller.clone(),
        library: ctx.library.clone(),
    })
}

/// Backward-compatible alias: look up only the param channel.
///
/// Used by `Vst3Processor` to drain the parameter queue each audio block.
pub fn lookup_vst3_channel(model_id: &str) -> Option<Vst3ParamChannel> {
    lookup_vst3_gui_context(model_id).map(|c| c.param_channel)
}
