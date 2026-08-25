//! Responsibility: maps every URI the plugin asks for to a stable URID.
//!
//! The LV2 URID extension: the plugin hands the host a URI string and gets an
//! integer back, and the same URI must always map to the same integer for the
//! lifetime of the instance.

use std::ffi::{c_char, c_void, CStr};

pub(crate) struct UridMap {
    uris: Vec<String>,
}

impl UridMap {
    pub(crate) fn new() -> Self {
        Self { uris: Vec::new() }
    }

    pub(crate) fn map(&mut self, uri: &str) -> u32 {
        if let Some(pos) = self.uris.iter().position(|u| u == uri) {
            return (pos + 1) as u32;
        }
        self.uris.push(uri.to_string());
        self.uris.len() as u32
    }
}

pub(crate) unsafe extern "C" fn urid_map_callback(handle: *mut c_void, uri: *const c_char) -> u32 {
    let map = unsafe { &mut *(handle as *mut UridMap) };
    let uri_str = unsafe { CStr::from_ptr(uri) }.to_str().unwrap_or("");
    map.map(uri_str)
}
