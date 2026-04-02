use anyhow::{bail, Context, Result};
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

// ---------------------------------------------------------------------------
// LV2 C ABI types (repr(C))
// ---------------------------------------------------------------------------

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
// Minimal URID map
// ---------------------------------------------------------------------------

const LV2_URID_MAP_URI: &str = "http://lv2plug.in/ns/ext/urid#map";
const LV2_BUF_SIZE_BOUNDED_URI: &str = "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
const LV2_OPTIONS_URI: &str = "http://lv2plug.in/ns/ext/options#options";
const LV2_BUF_SIZE_MAX_URI: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
const LV2_BUF_SIZE_MIN_URI: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const LV2_ATOM_INT_URI: &str = "http://lv2plug.in/ns/ext/atom#Int";

const LV2_WORKER_SCHEDULE_URI: &str = "http://lv2plug.in/ns/ext/worker#schedule";

/// LV2 Options context: instance-level option
const LV2_OPTIONS_INSTANCE: u32 = 0;

// ---------------------------------------------------------------------------
// LV2 Worker implementation (synchronous — executes work inline)
// ---------------------------------------------------------------------------

const LV2_WORKER_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/worker#interface";

/// LV2_Worker_Schedule — passed to plugin as a feature.
#[repr(C)]
struct LV2WorkerSchedule {
    handle: *mut c_void,
    schedule_work: Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> i32>,
}

/// LV2_Worker_Interface — retrieved from plugin via extension_data.
#[repr(C)]
struct LV2WorkerInterface {
    work: Option<unsafe extern "C" fn(
        instance: LV2Handle,
        respond: Option<unsafe extern "C" fn(handle: LV2Handle, size: u32, data: *const c_void) -> i32>,
        respond_handle: LV2Handle,
        size: u32,
        data: *const c_void,
    ) -> i32>,
    work_response: Option<unsafe extern "C" fn(instance: LV2Handle, size: u32, body: *const c_void) -> i32>,
    end_run: Option<unsafe extern "C" fn(instance: LV2Handle) -> i32>,
}

/// State shared between the schedule callback and the plugin instance.
struct WorkerState {
    handle: LV2Handle,
    worker_interface: *const LV2WorkerInterface,
}

/// Called by the plugin during run() to schedule work.
/// We execute it synchronously by calling work() + work_response() immediately.
unsafe extern "C" fn worker_schedule_callback(handle: *mut c_void, size: u32, data: *const c_void) -> i32 {
    let state = unsafe { &*(handle as *const WorkerState) };
    let iface = unsafe { &*state.worker_interface };

    if let Some(work_fn) = iface.work {
        // Call work() with a respond function that calls work_response()
        unsafe { work_fn(state.handle, Some(worker_respond_callback), state.handle, size, data) };
    }
    0 // LV2_WORKER_SUCCESS
}

/// Called by work() to send response back to the plugin.
unsafe extern "C" fn worker_respond_callback(handle: LV2Handle, _size: u32, _data: *const c_void) -> i32 {
    let _ = handle;
    0 // success — response will be delivered via work_response in end_run
}

/// LV2 Options Option struct (from lv2/options/options.h).
/// A null-terminated array of these is passed as the options feature data.
#[repr(C)]
struct LV2OptionsOption {
    context: u32,
    subject: u32,
    key: u32,
    size: u32,
    type_: u32,
    value: *const c_void,
}

struct UridMap {
    uris: Vec<String>,
}

impl UridMap {
    fn new() -> Self {
        Self { uris: Vec::new() }
    }

    fn map(&mut self, uri: &str) -> u32 {
        if let Some(pos) = self.uris.iter().position(|u| u == uri) {
            return (pos + 1) as u32;
        }
        self.uris.push(uri.to_string());
        self.uris.len() as u32
    }
}

unsafe extern "C" fn urid_map_callback(handle: *mut c_void, uri: *const c_char) -> u32 {
    let map = unsafe { &mut *(handle as *mut UridMap) };
    let uri_str = unsafe { CStr::from_ptr(uri) }
        .to_str()
        .unwrap_or("");
    map.map(uri_str)
}

// ---------------------------------------------------------------------------
// Port metadata (filled by the caller)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lv2PortKind {
    AudioIn,
    AudioOut,
    ControlIn,
    ControlOut,
}

#[derive(Debug, Clone)]
pub struct Lv2PortInfo {
    pub index: usize,
    pub kind: Lv2PortKind,
    pub name: String,
    pub default_value: f32,
}

// ---------------------------------------------------------------------------
// Lv2Plugin — owns the loaded library and instance
// ---------------------------------------------------------------------------

pub struct Lv2Plugin {
    _library: libloading::Library,
    descriptor: *const LV2Descriptor,
    handle: LV2Handle,
    // These must outlive the plugin instance:
    _urid_map: Box<UridMap>,
    _lv2_urid_map_struct: Box<LV2UridMap>,
    _features: Vec<LV2Feature>,
    _feature_ptrs: Vec<*const LV2Feature>,
    // Keep CStrings alive for the lifetime of the plugin
    _urid_map_uri_cstr: CString,
    _buf_size_uri_cstr: CString,
    _options_uri_cstr: CString,
    _worker_uri_cstr: CString,
    _worker_schedule: Box<LV2WorkerSchedule>,
    _worker_state: Option<Box<WorkerState>>,
    _options_array: Box<[LV2OptionsOption]>,
    _buf_size_values: Box<[i32; 2]>,
    _bundle_path_cstr: CString,
}

// The processor runs on a single audio thread; the plugin pointer is not
// shared across threads, so we assert Send. LV2 plugins are inherently
// single-threaded in their `run` path.
unsafe impl Send for Lv2Plugin {}
unsafe impl Sync for Lv2Plugin {}

impl Lv2Plugin {
    /// Load an LV2 plugin from a shared library.
    ///
    /// - `lib_path`: path to the `.dylib` / `.so` / `.dll`
    /// - `uri`: the LV2 plugin URI to look up in the descriptor list
    /// - `sample_rate`: audio sample rate
    /// - `bundle_path`: path to the `.lv2` bundle directory (TTL metadata)
    pub fn load(
        lib_path: &str,
        uri: &str,
        sample_rate: f64,
        bundle_path: &str,
    ) -> Result<Self> {
        // 1. dlopen
        let library = unsafe { libloading::Library::new(lib_path) }
            .with_context(|| format!("failed to load LV2 library: {lib_path}"))?;

        // 2. Get lv2_descriptor symbol
        let lv2_descriptor_fn: libloading::Symbol<
            unsafe extern "C" fn(index: u32) -> *const LV2Descriptor,
        > = unsafe { library.get(b"lv2_descriptor\0") }
            .context("symbol 'lv2_descriptor' not found in library")?;

        // 3. Find the descriptor matching the requested URI
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

        // 4. Set up URID map
        let mut urid_map = Box::new(UridMap::new());
        let mut lv2_urid_map_struct = Box::new(LV2UridMap {
            handle: urid_map.as_mut() as *mut UridMap as *mut c_void,
            map: Some(urid_map_callback),
        });

        let urid_map_uri_cstr = CString::new(LV2_URID_MAP_URI).unwrap();
        let buf_size_uri_cstr = CString::new(LV2_BUF_SIZE_BOUNDED_URI).unwrap();
        let options_uri_cstr = CString::new(LV2_OPTIONS_URI).unwrap();
        let worker_uri_cstr = CString::new(LV2_WORKER_SCHEDULE_URI).unwrap();

        // Worker schedule (no-op stub satisfying the feature requirement)
        let mut worker_schedule = Box::new(LV2WorkerSchedule {
            handle: ptr::null_mut(),
            schedule_work: Some(worker_schedule_callback),
        });

        // Buffer size values that must outlive the options array
        let buf_size_values: Box<[i32; 2]> = Box::new([4096i32, 1i32]); // [max, min]

        // Map URIs for options
        let max_block_urid = urid_map.map(LV2_BUF_SIZE_MAX_URI);
        let min_block_urid = urid_map.map(LV2_BUF_SIZE_MIN_URI);
        let atom_int_urid = urid_map.map(LV2_ATOM_INT_URI);

        // Options array with buffer size info (null-terminated)
        let options_array: Box<[LV2OptionsOption]> = Box::new([
            LV2OptionsOption {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: max_block_urid,
                size: 4, // sizeof(int32_t)
                type_: atom_int_urid,
                value: buf_size_values.as_ptr() as *const c_void, // points to [0] = 4096
            },
            LV2OptionsOption {
                context: LV2_OPTIONS_INSTANCE,
                subject: 0,
                key: min_block_urid,
                size: 4,
                type_: atom_int_urid,
                value: unsafe { buf_size_values.as_ptr().add(1) as *const c_void }, // points to [1] = 1
            },
            // Null terminator
            LV2OptionsOption {
                context: 0,
                subject: 0,
                key: 0,
                size: 0,
                type_: 0,
                value: ptr::null(),
            },
        ]);

        // 5. Build features array
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

        // Null-terminated array of pointers
        let mut feature_ptrs: Vec<*const LV2Feature> =
            features.iter().map(|f| f as *const LV2Feature).collect();
        feature_ptrs.push(ptr::null());

        // 6. Instantiate — LV2 spec requires bundle_path to end with '/'
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

        // 7. Set up worker if plugin supports it
        let worker_iface_uri = CString::new(LV2_WORKER_INTERFACE_URI).unwrap();
        let mut worker_state: Option<Box<WorkerState>> = None;
        if let Some(ext_data) = unsafe { (*descriptor).extension_data } {
            let iface_ptr = unsafe { ext_data(worker_iface_uri.as_ptr()) };
            if !iface_ptr.is_null() {
                let mut ws = Box::new(WorkerState {
                    handle,
                    worker_interface: iface_ptr as *const LV2WorkerInterface,
                });
                // Point the schedule handle to the worker state
                worker_schedule.handle = ws.as_mut() as *mut WorkerState as *mut c_void;
                worker_state = Some(ws);
                log::debug!("LV2 worker interface found for '{uri}'");
            }
        }

        // 8. Activate
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
            _buf_size_values: buf_size_values,
            _bundle_path_cstr: bundle_path_cstr,
        })
    }

    /// Connect a port to a data buffer.
    ///
    /// # Safety
    /// The caller must ensure `data` points to valid memory that outlives the
    /// next `run()` call and matches the port type.
    pub unsafe fn connect_port(&self, port_index: u32, data: *mut c_void) {
        if let Some(connect) = unsafe { (*self.descriptor).connect_port } {
            unsafe { connect(self.handle, port_index, data) };
        }
    }

    /// Run the plugin for `n_samples` frames.
    pub fn run(&self, n_samples: u32) {
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
