//! Shared channel for VST3 parameter updates from the plugin GUI to the audio thread.
//!
//! When the user moves a knob in the plugin's native editor window, the plugin
//! calls `IComponentHandler::performEdit`. We capture that call and push the
//! update through this lock-free queue so the audio thread can apply it without
//! any locking on the hot path.

use crossbeam_queue::SegQueue;
use std::sync::Arc;

/// A single parameter change sent by the plugin's GUI.
#[derive(Debug, Clone, Copy)]
pub struct Vst3ParamUpdate {
    /// Plugin parameter ID (from `ParameterInfo::id`).
    pub id: u32,
    /// Normalized value in the range 0.0..=1.0.
    pub normalized: f64,
}

/// A lock-free channel that carries `Vst3ParamUpdate` messages from the GUI
/// thread (producer) to the audio thread (consumer).
pub type Vst3ParamChannel = Arc<SegQueue<Vst3ParamUpdate>>;

/// Create a new empty parameter channel.
pub fn vst3_param_channel() -> Vst3ParamChannel {
    Arc::new(SegQueue::new())
}
