//! #780 regression guard: the host `IComponentHandler` MUST be registered on
//! the controller BEFORE `createView`. If it is set afterwards the plugin's
//! editor view ignores it, `performEdit` from the native GUI goes nowhere, and
//! knob moves are silent (no audio change, no dirty flag) — the exact bug the
//! owner hit. This is headless: a fake `IEditController` records the call order
//! and `createView` returns null (so the real function would `bail!` before any
//! AppKit window is created), so we never open a GUI.
//!
//! macOS-only because `register_handler_then_create_view` is macOS-gated.

use std::sync::{Arc, Mutex};

use vst3::Steinberg::Vst::{
    IComponentHandler, IEditController, IEditControllerTrait, ParamID, ParamValue, ParameterInfo,
    String128, TChar,
};
use vst3::Steinberg::{
    int32, kResultOk, tresult, FIDString, FUnknown, IBStream, IPlugView, IPluginBaseTrait,
};
use vst3::{Class, ComPtr, ComWrapper};

use crate::editor::register_handler_then_create_view;
use crate::param_channel::vst3_param_channel;

/// A minimal `IEditController` that records the order of the two calls the fix
/// cares about and returns a null view from `createView`.
struct OrderRecordingController {
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl Class for OrderRecordingController {
    type Interfaces = (IEditController,);
}

impl IPluginBaseTrait for OrderRecordingController {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

#[allow(non_snake_case)]
impl IEditControllerTrait for OrderRecordingController {
    unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
    unsafe fn getParameterCount(&self) -> int32 {
        0
    }
    unsafe fn getParameterInfo(&self, _index: int32, _info: *mut ParameterInfo) -> tresult {
        kResultOk
    }
    unsafe fn getParamStringByValue(
        &self,
        _id: ParamID,
        _value: ParamValue,
        _string: *mut String128,
    ) -> tresult {
        kResultOk
    }
    unsafe fn getParamValueByString(
        &self,
        _id: ParamID,
        _string: *mut TChar,
        _value: *mut ParamValue,
    ) -> tresult {
        kResultOk
    }
    unsafe fn normalizedParamToPlain(&self, _id: ParamID, value: ParamValue) -> ParamValue {
        value
    }
    unsafe fn plainParamToNormalized(&self, _id: ParamID, value: ParamValue) -> ParamValue {
        value
    }
    unsafe fn getParamNormalized(&self, _id: ParamID) -> ParamValue {
        0.0
    }
    unsafe fn setParamNormalized(&self, _id: ParamID, _value: ParamValue) -> tresult {
        kResultOk
    }
    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        self.calls.lock().unwrap().push("setComponentHandler");
        kResultOk
    }
    unsafe fn createView(&self, _name: FIDString) -> *mut IPlugView {
        self.calls.lock().unwrap().push("createView");
        std::ptr::null_mut()
    }
}

#[test]
fn component_handler_is_registered_before_create_view() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let wrapper = ComWrapper::new(OrderRecordingController {
        calls: calls.clone(),
    });
    let controller: ComPtr<IEditController> = wrapper
        .to_com_ptr()
        .expect("fake controller exposes IEditController");

    let (view_ptr, _handler) = register_handler_then_create_view(&controller, vst3_param_channel());

    assert!(
        view_ptr.is_null(),
        "fake createView returns null → no window"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["setComponentHandler", "createView"],
        "the host must call setComponentHandler BEFORE createView, or the native \
         editor's performEdit never reaches the audio processor (#780)"
    );
}
