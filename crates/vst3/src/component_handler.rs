//! Host-side `IComponentHandler` implementation.
//!
//! The VST3 spec requires the host to provide an `IComponentHandler` to the
//! `IEditController` so that the plugin can notify the host of parameter
//! changes made from its own GUI. This module implements that interface and
//! forwards each `performEdit` call onto a lock-free queue that the audio
//! thread drains before each processing block.

use vst3::{
    Class, ComWrapper,
    Steinberg::Vst::{IComponentHandler, IComponentHandlerTrait, ParamID, ParamValue},
    Steinberg::kResultOk,
};

use crate::param_channel::Vst3ParamChannel;
use crate::param_channel::Vst3ParamUpdate;

/// Host-side component handler: implements `IComponentHandler` and forwards
/// parameter edits to the audio thread via a lock-free queue.
pub struct ComponentHandler {
    channel: Vst3ParamChannel,
}

impl ComponentHandler {
    pub fn new(channel: Vst3ParamChannel) -> Self {
        Self { channel }
    }

    /// Wrap `self` in a `ComWrapper` and return a raw pointer suitable for
    /// passing to `IEditController::setComponentHandler`.
    ///
    /// # Safety
    /// The returned pointer is valid as long as the `ComWrapper` is alive.
    /// Callers must ensure the `ComWrapper` outlives any plugin usage of the
    /// pointer.
    pub fn into_com_ptr(self) -> ComWrapper<ComponentHandler> {
        ComWrapper::new(self)
    }
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> i32 {
        kResultOk
    }

    #[allow(non_snake_case)]
    unsafe fn performEdit(&self, id: ParamID, valueNormalized: ParamValue) -> i32 {
        self.channel.push(Vst3ParamUpdate { id, normalized: valueNormalized });
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> i32 {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: i32) -> i32 {
        kResultOk
    }
}
