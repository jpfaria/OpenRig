//! Responsibility: mirrors the LV2 C ABI the host talks to.
//!
//! `repr(C)` twins of the structs `lv2.h` declares, plus the extension URIs
//! the host asks a plugin for. Nothing here has behaviour — it is the shape
//! of the contract with the plugin's shared library.

use std::ffi::{c_char, c_void};

pub type LV2Handle = *mut c_void;

#[repr(C)]
pub struct LV2Descriptor {
    pub uri: *const c_char,
    pub instantiate: Option<
        unsafe extern "C" fn(
            descriptor: *const LV2Descriptor,
            sample_rate: f64,
            bundle_path: *const c_char,
            features: *const *const LV2Feature,
        ) -> LV2Handle,
    >,
    pub connect_port:
        Option<unsafe extern "C" fn(instance: LV2Handle, port: u32, data_location: *mut c_void)>,
    pub activate: Option<unsafe extern "C" fn(instance: LV2Handle)>,
    pub run: Option<unsafe extern "C" fn(instance: LV2Handle, n_samples: u32)>,
    pub deactivate: Option<unsafe extern "C" fn(instance: LV2Handle)>,
    pub cleanup: Option<unsafe extern "C" fn(instance: LV2Handle)>,
    pub extension_data: Option<unsafe extern "C" fn(uri: *const c_char) -> *const c_void>,
}

#[repr(C)]
pub struct LV2Feature {
    pub uri: *const c_char,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct LV2UridMap {
    pub handle: *mut c_void,
    pub map: Option<unsafe extern "C" fn(handle: *mut c_void, uri: *const c_char) -> u32>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub(crate) const LV2_URID_MAP_URI: &str = "http://lv2plug.in/ns/ext/urid#map";
pub(crate) const LV2_BUF_SIZE_BOUNDED_URI: &str =
    "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
pub(crate) const LV2_OPTIONS_URI: &str = "http://lv2plug.in/ns/ext/options#options";
pub(crate) const LV2_BUF_SIZE_MAX_URI: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
pub(crate) const LV2_BUF_SIZE_MIN_URI: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
pub(crate) const LV2_ATOM_INT_URI: &str = "http://lv2plug.in/ns/ext/atom#Int";
pub(crate) const LV2_ATOM_FLOAT_URI: &str = "http://lv2plug.in/ns/ext/atom#Float";
pub(crate) const LV2_PARAM_SAMPLE_RATE_URI: &str = "http://lv2plug.in/ns/ext/parameters#sampleRate";
pub(crate) const LV2_WORKER_SCHEDULE_URI: &str = "http://lv2plug.in/ns/ext/worker#schedule";
pub(crate) const LV2_WORKER_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/worker#interface";

// ---------------------------------------------------------------------------
// LV2 Options
// ---------------------------------------------------------------------------

#[repr(C)]
pub(crate) struct LV2OptionsOption {
    pub(crate) context: u32,
    pub(crate) subject: u32,
    pub(crate) key: u32,
    pub(crate) size: u32,
    pub(crate) type_: u32,
    pub(crate) value: *const c_void,
}
