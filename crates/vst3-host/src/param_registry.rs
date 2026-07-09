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
use vst3::Steinberg::kResultOk;
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ParameterInfo};

use crate::host::Vst3ParamInfo;
use crate::host_utils::char16_array_to_string;
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
    /// The plugin's catalog model id. The registry is keyed by a per-block
    /// instance key (#780), so this carries the model needed to resolve the
    /// catalog entry (display name) when the GUI opens the editor.
    pub model_id: String,
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

/// Register a GUI context under `instance_key`, replacing any previous entry.
///
/// `instance_key` is a per-block identity (the `BlockId`, #780) so two blocks
/// that reference the same plugin `model_id` each keep their own controller
/// instead of the last-registered one clobbering the first. `model_id` is
/// stored so the GUI can resolve the catalog entry when it opens the editor.
///
/// Called by the engine after loading the plugin. Returns the newly created
/// `Vst3ParamChannel` so the caller can attach it to its processor instance.
pub fn register_vst3_gui_context(
    instance_key: &str,
    model_id: &str,
    controller: ComPtr<IEditController>,
    library: Arc<libloading::Library>,
) -> Vst3ParamChannel {
    let channel = vst3_param_channel();
    registry()
        .write()
        .expect("vst3 param registry poisoned")
        .insert(
            instance_key.to_string(),
            Vst3GuiContext {
                param_channel: channel.clone(),
                controller,
                library,
                model_id: model_id.to_string(),
            },
        );
    log::info!(
        "VST3 registry: registered context for instance '{}' (model '{}')",
        instance_key,
        model_id
    );
    channel
}

/// Look up the `Vst3GuiContext` previously registered under `instance_key`.
///
/// Returns `None` if no processor for this block instance has been built yet.
/// The returned context clones the COM pointer and `Arc` (cheap ref-count bumps).
pub fn lookup_vst3_gui_context(instance_key: &str) -> Option<Vst3GuiContext> {
    let guard = registry().read().expect("vst3 param registry poisoned");
    let ctx = guard.get(instance_key)?;
    Some(Vst3GuiContext {
        param_channel: ctx.param_channel.clone(),
        controller: ctx.controller.clone(),
        library: ctx.library.clone(),
        model_id: ctx.model_id.clone(),
    })
}

/// Backward-compatible alias: look up only the param channel.
///
/// Used by `Vst3Processor` to drain the parameter queue each audio block.
pub fn lookup_vst3_channel(instance_key: &str) -> Option<Vst3ParamChannel> {
    lookup_vst3_gui_context(instance_key).map(|c| c.param_channel)
}

/// Read the current normalized value of every parameter whose value differs
/// from its controller default (> 1e-6), for the instance registered under
/// `instance_key`. Returns `None` when no context is registered (the plugin is
/// not live). Persisted by the save path as `p{id}` percent so native-editor
/// edits survive save + reload (#780).
///
/// Main/save-thread only — reads the controller under the registry lock; never
/// call from the audio thread.
pub fn capture_vst3_params(instance_key: &str) -> Option<Vec<(u32, f64)>> {
    let guard = registry().read().expect("vst3 param registry poisoned");
    let ctx = guard.get(instance_key)?;
    let count = unsafe { ctx.controller.getParameterCount() };
    let mut out = Vec::new();
    for i in 0..count {
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        if unsafe { ctx.controller.getParameterInfo(i, &mut info) } != kResultOk {
            continue;
        }
        let current = unsafe { ctx.controller.getParamNormalized(info.id) };
        if (current - info.defaultNormalizedValue).abs() > 1e-6 {
            out.push((info.id, current));
        }
    }
    Some(out)
}

/// Read a controller's full parameter metadata (id, title, default, …).
///
/// Main-thread only (walks the COM controller under the registry lock).
fn read_controller_params(controller: &ComPtr<IEditController>) -> Vec<Vst3ParamInfo> {
    let count = unsafe { controller.getParameterCount() };
    let mut out = Vec::with_capacity(count.max(0) as usize);
    for i in 0..count {
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        if unsafe { controller.getParameterInfo(i, &mut info) } != kResultOk {
            continue;
        }
        out.push(Vst3ParamInfo {
            id: info.id,
            title: char16_array_to_string(&info.title),
            short_title: char16_array_to_string(&info.shortTitle),
            units: char16_array_to_string(&info.units),
            step_count: info.stepCount,
            default_normalized: info.defaultNormalizedValue,
        });
    }
    out
}

/// The parameter metadata of any LIVE instance of `model_id` (from the first
/// registered context that matches), or `None` when no instance is loaded.
///
/// Reading from a live instance avoids loading a SECOND instance of a model
/// whose first is streaming — that concurrent `createInstance` vs `process()`
/// is the #779 crash. Used by the catalog to synthesise OpenRig knobs (#780).
pub fn live_params_for_model(model_id: &str) -> Option<Vec<Vst3ParamInfo>> {
    let guard = registry().read().expect("vst3 param registry poisoned");
    let ctx = guard.values().find(|c| c.model_id == model_id)?;
    Some(read_controller_params(&ctx.controller))
}
