//! Global registry that maps VST3 `model_id` → `Vst3ParamChannel`.
//!
//! The engine registers a channel when it builds a `Vst3Processor`.  When the
//! GUI opens the native editor it looks up the same channel so both ends share
//! the same `Arc<SegQueue>`.  This avoids any direct coupling between the
//! engine and the GUI layer.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::param_channel::{vst3_param_channel, Vst3ParamChannel};

static REGISTRY: OnceLock<RwLock<HashMap<String, Vst3ParamChannel>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, Vst3ParamChannel>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a new channel for `model_id`, replacing any previous entry.
///
/// Returns the newly created channel so the caller can attach it to its
/// processor instance.
pub fn register_vst3_channel(model_id: &str) -> Vst3ParamChannel {
    let channel = vst3_param_channel();
    registry()
        .write()
        .expect("vst3 param registry poisoned")
        .insert(model_id.to_string(), channel.clone());
    channel
}

/// Look up the channel previously registered for `model_id`.
///
/// Returns `None` if no processor for this model has been built yet.
pub fn lookup_vst3_channel(model_id: &str) -> Option<Vst3ParamChannel> {
    registry()
        .read()
        .expect("vst3 param registry poisoned")
        .get(model_id)
        .cloned()
}
