//! Global registry that maps a VST3 block-instance key → `Vst3GuiContext`.
//!
//! The engine registers a context (param channel + shared controller + library
//! Arc) when it builds a `Vst3Processor`. The catalog reads a live instance's
//! parameter metadata from here (`live_params_for_model`) to synthesise OpenRig
//! knobs without loading a second instance of a streaming plugin (#780).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use vst3::ComPtr;
use vst3::Steinberg::kResultOk;
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ParameterInfo, String128};

use crate::host::Vst3ParamInfo;
use crate::host_utils::char16_array_to_string;
use crate::param_channel::{vst3_param_channel, Vst3ParamChannel};

/// A live VST3 instance's shared handles, kept so the catalog can read its
/// parameter metadata (for OpenRig knobs) without a second load.
pub struct Vst3GuiContext {
    /// Lock-free queue shared with the audio processor (drained each block).
    pub param_channel: Vst3ParamChannel,
    /// Reference-counted pointer to the controller held by the audio processor,
    /// used to read parameter metadata.
    pub controller: ComPtr<IEditController>,
    /// Keeps the plugin dylib alive alongside the audio processor instance.
    pub library: Arc<libloading::Library>,
    /// The plugin's catalog model id. The registry is keyed by a per-block
    /// instance key (#780); this carries the model for `live_params_for_model`.
    pub model_id: String,
}

// SAFETY: `ComPtr<IEditController>` is a reference-counted COM pointer, only
// used on the main thread. `Arc<libloading::Library>` is inherently Send+Sync.
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

/// Read a controller's full parameter metadata (id, title, default, and — for
/// discrete `step_count >= 2` selects — the per-step `(value_percent, label)`
/// options read via `getParamStringByValue`).
///
/// Main-thread only (walks the COM controller).
pub(crate) fn read_controller_params(controller: &ComPtr<IEditController>) -> Vec<Vst3ParamInfo> {
    let count = unsafe { controller.getParameterCount() };
    let mut out = Vec::with_capacity(count.max(0) as usize);
    for i in 0..count {
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        if unsafe { controller.getParameterInfo(i, &mut info) } != kResultOk {
            continue;
        }
        // Read the step labels for every discrete param (>= 1). The schema uses
        // them both for selects and for the on/off-vs-selector heuristic (#780).
        let enum_options = if info.stepCount >= 1 {
            read_enum_options(controller, info.id, info.stepCount)
        } else {
            Vec::new()
        };
        out.push(Vst3ParamInfo {
            id: info.id,
            title: char16_array_to_string(&info.title),
            short_title: char16_array_to_string(&info.shortTitle),
            units: char16_array_to_string(&info.units),
            step_count: info.stepCount,
            default_normalized: info.defaultNormalizedValue,
            enum_options,
        });
    }
    out
}

/// For a discrete parameter, ask the controller for each step's display string.
/// Step `k` maps to normalized `k / step_count`; its stored value is that
/// normalized value as a percent string so the engine's `p{id}` percent → VST3
/// normalized conversion applies uniformly (#780).
fn read_enum_options(
    controller: &ComPtr<IEditController>,
    id: u32,
    step_count: i32,
) -> Vec<(String, String)> {
    (0..=step_count)
        .map(|k| {
            let normalized = k as f64 / step_count as f64;
            let mut buf: String128 = [0; 128];
            let label = if unsafe {
                controller.getParamStringByValue(id, normalized, &mut buf as *mut String128)
            } == kResultOk
            {
                char16_array_to_string(&buf)
            } else {
                String::new()
            };
            let label = if label.is_empty() {
                format!("{k}")
            } else {
                label
            };
            (format!("{}", normalized * 100.0), label)
        })
        .collect()
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
