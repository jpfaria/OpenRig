//! Host-side `IHostApplication` implementation.
//!
//! The VST3 spec says the host must pass an `IHostApplication` as the context
//! to `IPluginBase::initialize`. We used to pass null. Plugins that build on a
//! GUI framework (JUCE) then fall back to grabbing the process `NSApplication`
//! themselves on first init, which makes a *second* `createInstance` fail with
//! result=-1 once the host already owns an event loop — the "VST3 não entra na
//! chain" symptom (#251). Passing a real host context avoids that fallback.

use std::ffi::c_void;

use vst3::{
    Class, ComWrapper,
    Steinberg::Vst::{IHostApplication, IHostApplicationTrait, String128},
    Steinberg::{kNotImplemented, kResultOk, tresult, TUID},
};

/// Minimal `IHostApplication`: reports the host name and declines to create
/// helper objects (plugins fall back to their own when we return not-implemented).
pub struct HostApplication;

impl HostApplication {
    pub fn new() -> ComWrapper<HostApplication> {
        ComWrapper::new(HostApplication)
    }
}

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if !name.is_null() {
            let buf = &mut *name;
            let mut i = 0usize;
            for u in "OpenRig".encode_utf16() {
                if i >= buf.len() - 1 {
                    break;
                }
                buf[i] = u;
                i += 1;
            }
            buf[i] = 0;
        }
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        _obj: *mut *mut c_void,
    ) -> tresult {
        kNotImplemented
    }
}
