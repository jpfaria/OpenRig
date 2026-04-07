//! Host-side IParameterChanges / IParamValueQueue COM objects.
//!
//! The VST3 spec requires the host to pass parameter changes to the audio
//! processor via `ProcessData::inputParameterChanges`.  Simply calling
//! `IEditController::setParamNormalized` only updates the controller's display
//! state; it does NOT reach the audio DSP for plugins with a separate
//! component object.
//!
//! This module provides minimal read-only implementations that the plugin can
//! call during `IAudioProcessor::process()`.

use std::cell::Cell;
use std::ptr;

use vst3::{
    Class, ComWrapper,
    Steinberg::Vst::{
        IParamValueQueue, IParamValueQueueTrait,
        IParameterChanges, IParameterChangesTrait,
        ParamID, ParamValue,
    },
    Steinberg::{kResultOk, kInvalidArgument},
};

// ---------------------------------------------------------------------------
// IParamValueQueue — one queue per parameter
// ---------------------------------------------------------------------------

/// Single-point parameter queue: holds one (sample_offset=0, value) pair.
pub struct HostParamValueQueue {
    id: u32,
    value: Cell<f64>,
}

impl HostParamValueQueue {
    pub fn new(id: u32, value: f64) -> Self {
        Self { id, value: Cell::new(value) }
    }
}

// SAFETY: HostParamValueQueue is only used from the audio thread (single
// consumer), but ComWrapper requires Send+Sync for the impl. We assert this
// manually because Cell<f64> is !Send by default.
unsafe impl Send for HostParamValueQueue {}
unsafe impl Sync for HostParamValueQueue {}

impl Class for HostParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for HostParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id
    }

    unsafe fn getPointCount(&self) -> i32 {
        1
    }

    #[allow(non_snake_case)]
    unsafe fn getPoint(
        &self,
        index: i32,
        sampleOffset: *mut i32,
        value: *mut ParamValue,
    ) -> i32 {
        if index != 0 {
            return kInvalidArgument;
        }
        *sampleOffset = 0;
        *value = self.value.get();
        kResultOk
    }

    #[allow(non_snake_case)]
    unsafe fn addPoint(
        &self,
        _sampleOffset: i32,
        value: ParamValue,
        _index: *mut i32,
    ) -> i32 {
        // Allow the plugin to write back its current value (some plugins do this).
        self.value.set(value);
        kResultOk
    }
}

// ---------------------------------------------------------------------------
// IParameterChanges — one collection per process() call
// ---------------------------------------------------------------------------

/// Collection of per-parameter queues passed as `inputParameterChanges`.
pub struct HostParameterChanges {
    queues: Vec<ComWrapper<HostParamValueQueue>>,
}

impl HostParameterChanges {
    /// Build from a slice of `(param_id, normalized_value)` pairs.
    pub fn new(params: &[(u32, f64)]) -> Self {
        Self {
            queues: params
                .iter()
                .map(|&(id, v)| ComWrapper::new(HostParamValueQueue::new(id, v)))
                .collect(),
        }
    }

}

unsafe impl Send for HostParameterChanges {}
unsafe impl Sync for HostParameterChanges {}

impl Class for HostParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for HostParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.queues.len() as i32
    }

    #[allow(non_snake_case)]
    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        self.queues
            .get(index as usize)
            .and_then(|q| q.as_com_ref::<IParamValueQueue>())
            .map(|r| r.as_ptr())
            .unwrap_or(ptr::null_mut())
    }

    #[allow(non_snake_case)]
    unsafe fn addParameterData(
        &self,
        _id: *const ParamID,
        _index: *mut i32,
    ) -> *mut IParamValueQueue {
        // Host-provided; plugin should not add data to our input changes.
        ptr::null_mut()
    }
}
