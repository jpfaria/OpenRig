//! Responsibility: hosts one live LV2 plugin instance.
//!
//! Owns the `dlopen`ed library, the instantiated handle and every feature
//! block the plugin was given at instantiation (they must outlive it), and
//! drives `run` on the audio thread.

use anyhow::{bail, Context, Result};
use std::ffi::{c_void, CStr, CString};
use std::ptr;

use crate::host_abi::{
    LV2Descriptor, LV2Feature, LV2Handle, LV2OptionsOption, LV2UridMap, LV2_ATOM_FLOAT_URI,
    LV2_ATOM_INT_URI, LV2_BUF_SIZE_BOUNDED_URI, LV2_BUF_SIZE_MAX_URI, LV2_BUF_SIZE_MIN_URI,
    LV2_OPTIONS_URI, LV2_PARAM_SAMPLE_RATE_URI, LV2_URID_MAP_URI, LV2_WORKER_INTERFACE_URI,
    LV2_WORKER_SCHEDULE_URI,
};
use crate::host_urid::{urid_map_callback, UridMap};
use crate::host_worker::{
    worker_schedule_callback, LV2WorkerInterface, LV2WorkerSchedule, WorkerState,
};

pub struct Lv2Plugin {
    _library: libloading::Library,
    descriptor: *const LV2Descriptor,
    handle: LV2Handle,
    _urid_map: Box<UridMap>,
    _lv2_urid_map_struct: Box<LV2UridMap>,
    _features: Vec<LV2Feature>,
    _feature_ptrs: Vec<*const LV2Feature>,
    _urid_map_uri_cstr: CString,
    _buf_size_uri_cstr: CString,
    _options_uri_cstr: CString,
    _worker_uri_cstr: CString,
    _worker_schedule: Box<LV2WorkerSchedule>,
    _worker_state: Option<Box<WorkerState>>,
    _options_array: Vec<LV2OptionsOption>,
    _options_min_block: Box<i32>,
    _options_max_block: Box<i32>,
    _options_sample_rate: Box<f32>,
    _bundle_path_cstr: CString,
}

unsafe impl Send for Lv2Plugin {}
unsafe impl Sync for Lv2Plugin {}

impl Lv2Plugin {
    pub fn load(lib_path: &str, uri: &str, sample_rate: f64, bundle_path: &str) -> Result<Self> {
        // 1. dlopen
        let library = unsafe { libloading::Library::new(lib_path) }
            .with_context(|| format!("failed to load LV2 library: {lib_path}"))?;

        // 2. Get lv2_descriptor symbol
        let lv2_descriptor_fn: libloading::Symbol<
            unsafe extern "C" fn(index: u32) -> *const LV2Descriptor,
        > = unsafe { library.get(b"lv2_descriptor\0") }
            .context("symbol 'lv2_descriptor' not found in library")?;

        // 3. Find descriptor matching URI
        let mut descriptor: *const LV2Descriptor = ptr::null();
        for idx in 0..128 {
            let desc = unsafe { lv2_descriptor_fn(idx) };
            if desc.is_null() {
                break;
            }
            let desc_uri = unsafe { CStr::from_ptr((*desc).uri) }
                .to_str()
                .unwrap_or("");
            if desc_uri == uri {
                descriptor = desc;
                break;
            }
        }
        if descriptor.is_null() {
            bail!("LV2 plugin URI '{uri}' not found in {lib_path}");
        }

        // 4. Set up URID map and pre-map option URIDs
        let mut urid_map = Box::new(UridMap::new());
        let urid_atom_int = urid_map.map(LV2_ATOM_INT_URI);
        let urid_atom_float = urid_map.map(LV2_ATOM_FLOAT_URI);
        let urid_sample_rate_key = urid_map.map(LV2_PARAM_SAMPLE_RATE_URI);
        let urid_min_block_key = urid_map.map(LV2_BUF_SIZE_MIN_URI);
        let urid_max_block_key = urid_map.map(LV2_BUF_SIZE_MAX_URI);

        let mut lv2_urid_map_struct = Box::new(LV2UridMap {
            handle: urid_map.as_mut() as *mut UridMap as *mut c_void,
            map: Some(urid_map_callback),
        });

        // 5. Build options with stable heap addresses
        let options_min_block = Box::new(1i32);
        let options_max_block = Box::new(4096i32);
        let options_sample_rate = Box::new(sample_rate as f32);

        let options_array: Vec<LV2OptionsOption> = vec![
            LV2OptionsOption {
                context: 0,
                subject: 0,
                key: urid_sample_rate_key,
                size: 4,
                type_: urid_atom_float,
                value: options_sample_rate.as_ref() as *const f32 as *const c_void,
            },
            LV2OptionsOption {
                context: 0,
                subject: 0,
                key: urid_min_block_key,
                size: 4,
                type_: urid_atom_int,
                value: options_min_block.as_ref() as *const i32 as *const c_void,
            },
            LV2OptionsOption {
                context: 0,
                subject: 0,
                key: urid_max_block_key,
                size: 4,
                type_: urid_atom_int,
                value: options_max_block.as_ref() as *const i32 as *const c_void,
            },
            // Terminator
            LV2OptionsOption {
                context: 0,
                subject: 0,
                key: 0,
                size: 0,
                type_: 0,
                value: ptr::null(),
            },
        ];

        // 6. Build CStrings and features
        let urid_map_uri_cstr = CString::new(LV2_URID_MAP_URI).unwrap();
        let buf_size_uri_cstr = CString::new(LV2_BUF_SIZE_BOUNDED_URI).unwrap();
        let options_uri_cstr = CString::new(LV2_OPTIONS_URI).unwrap();
        let worker_uri_cstr = CString::new(LV2_WORKER_SCHEDULE_URI).unwrap();

        let mut worker_schedule = Box::new(LV2WorkerSchedule {
            handle: ptr::null_mut(),
            schedule_work: Some(worker_schedule_callback),
        });

        let features = vec![
            LV2Feature {
                uri: urid_map_uri_cstr.as_ptr(),
                data: lv2_urid_map_struct.as_mut() as *mut LV2UridMap as *mut c_void,
            },
            LV2Feature {
                uri: buf_size_uri_cstr.as_ptr(),
                data: ptr::null_mut(),
            },
            LV2Feature {
                uri: options_uri_cstr.as_ptr(),
                data: options_array.as_ptr() as *mut c_void,
            },
            LV2Feature {
                uri: worker_uri_cstr.as_ptr(),
                data: worker_schedule.as_mut() as *mut LV2WorkerSchedule as *mut c_void,
            },
        ];

        let mut feature_ptrs: Vec<*const LV2Feature> =
            features.iter().map(|f| f as *const LV2Feature).collect();
        feature_ptrs.push(ptr::null());

        // 7. Instantiate — LV2 spec requires bundle_path to end with '/'
        let bundle_with_slash = if bundle_path.ends_with('/') {
            bundle_path.to_string()
        } else {
            format!("{}/", bundle_path)
        };
        let bundle_path_cstr =
            CString::new(bundle_with_slash).context("invalid bundle_path for C string")?;

        let instantiate = unsafe { (*descriptor).instantiate }
            .context("LV2 descriptor has no instantiate function")?;

        let handle = unsafe {
            instantiate(
                descriptor,
                sample_rate,
                bundle_path_cstr.as_ptr(),
                feature_ptrs.as_ptr(),
            )
        };

        if handle.is_null() {
            bail!("LV2 plugin instantiate returned null for URI '{uri}'");
        }

        // 8. Set up worker if plugin supports it
        let worker_iface_uri = CString::new(LV2_WORKER_INTERFACE_URI).unwrap();
        let mut worker_state: Option<Box<WorkerState>> = None;
        if let Some(ext_data) = unsafe { (*descriptor).extension_data } {
            let iface_ptr = unsafe { ext_data(worker_iface_uri.as_ptr()) };
            if !iface_ptr.is_null() {
                let mut ws = WorkerState::new(handle, iface_ptr as *const LV2WorkerInterface);
                worker_schedule.handle = ws.as_mut() as *mut WorkerState as *mut c_void;
                worker_state = Some(ws);
                log::debug!("LV2 worker interface found for '{uri}'");
            }
        }

        // 9. Activate
        if let Some(activate) = unsafe { (*descriptor).activate } {
            unsafe { activate(handle) };
        }

        log::info!("LV2 plugin loaded: {uri} from {lib_path}");

        Ok(Self {
            _library: library,
            descriptor,
            handle,
            _urid_map: urid_map,
            _lv2_urid_map_struct: lv2_urid_map_struct,
            _features: features,
            _feature_ptrs: feature_ptrs,
            _urid_map_uri_cstr: urid_map_uri_cstr,
            _buf_size_uri_cstr: buf_size_uri_cstr,
            _options_uri_cstr: options_uri_cstr,
            _worker_uri_cstr: worker_uri_cstr,
            _worker_schedule: worker_schedule,
            _worker_state: worker_state,
            _options_array: options_array,
            _options_min_block: options_min_block,
            _options_max_block: options_max_block,
            _options_sample_rate: options_sample_rate,
            _bundle_path_cstr: bundle_path_cstr,
        })
    }

    pub(crate) unsafe fn connect_port(&self, port_index: u32, data: *mut c_void) {
        if let Some(connect) = unsafe { (*self.descriptor).connect_port } {
            unsafe { connect(self.handle, port_index, data) };
        }
    }

    pub fn run(&self, n_samples: u32) {
        // Issue #670: deliver any completed worker responses to the plugin
        // (work_response + end_run) before it processes this block — the
        // async worker thread produced them since the last run().
        if let Some(ws) = &self._worker_state {
            ws.deliver_responses();
        }
        if let Some(run_fn) = unsafe { (*self.descriptor).run } {
            unsafe { run_fn(self.handle, n_samples) };
        }
    }
}

impl Drop for Lv2Plugin {
    fn drop(&mut self) {
        unsafe {
            if let Some(deactivate) = (*self.descriptor).deactivate {
                deactivate(self.handle);
            }
            if let Some(cleanup) = (*self.descriptor).cleanup {
                cleanup(self.handle);
            }
        }
        log::debug!("LV2 plugin instance cleaned up");
    }
}
