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
const LV2_ATOM_INT_URI: &str = "http://lv2plug.in/ns/ext/atom#Int";
const LV2_ATOM_FLOAT_URI: &str = "http://lv2plug.in/ns/ext/atom#Float";
const LV2_PARAM_SAMPLE_RATE_URI: &str = "http://lv2plug.in/ns/ext/parameters#sampleRate";
const LV2_BUFSZ_MIN_BLOCK_URI: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const LV2_BUFSZ_MAX_BLOCK_URI: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";

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
// LV2 Options extension types
// ---------------------------------------------------------------------------

/// Corresponds to LV2_Options_Option in C.
#[repr(C)]
struct LV2OptionsOption {
    context: u32,
    subject: u32,
    key: u32,
    size: u32,
    type_: u32,
    value: *const c_void,
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
    _bundle_path_cstr: CString,
    // Options feature data — stable heap addresses pointed to by _options_array
    _options_min_block: Box<i32>,
    _options_max_block: Box<i32>,
    _options_sample_rate: Box<f32>,
    _options_array: Vec<LV2OptionsOption>,
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

        // 4. Set up URID map and pre-map option URIDs
        let mut urid_map = Box::new(UridMap::new());

        // Pre-map URIDs used by the Options feature. These must be mapped
        // before the features array is built so that plugins querying the map
        // for these URIs get the same IDs as the values we embed in the options.
        let urid_atom_int = urid_map.map(LV2_ATOM_INT_URI);
        let urid_atom_float = urid_map.map(LV2_ATOM_FLOAT_URI);
        let urid_sample_rate_key = urid_map.map(LV2_PARAM_SAMPLE_RATE_URI);
        let urid_min_block_key = urid_map.map(LV2_BUFSZ_MIN_BLOCK_URI);
        let urid_max_block_key = urid_map.map(LV2_BUFSZ_MAX_BLOCK_URI);

        let mut lv2_urid_map_struct = Box::new(LV2UridMap {
            handle: urid_map.as_mut() as *mut UridMap as *mut c_void,
            map: Some(urid_map_callback),
        });

        // 5. Build options data with stable heap addresses
        let options_min_block = Box::new(1i32);
        let options_max_block = Box::new(4096i32);
        let options_sample_rate = Box::new(sample_rate as f32);

        // Null-terminated array of LV2_Options_Option (context=0 means Instance)
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
            LV2OptionsOption { context: 0, subject: 0, key: 0, size: 0, type_: 0, value: ptr::null() },
        ];

        let urid_map_uri_cstr = CString::new(LV2_URID_MAP_URI).unwrap();
        let buf_size_uri_cstr = CString::new(LV2_BUF_SIZE_BOUNDED_URI).unwrap();
        let options_uri_cstr = CString::new(LV2_OPTIONS_URI).unwrap();

        // 6. Build features array (urid#map, buf-size, options)
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
                data: options_array.as_ptr() as *mut LV2OptionsOption as *mut c_void,
            },
        ];

        // Null-terminated array of pointers
        let mut feature_ptrs: Vec<*const LV2Feature> =
            features.iter().map(|f| f as *const LV2Feature).collect();
        feature_ptrs.push(ptr::null());

        // 7. Instantiate
        let bundle_path_cstr =
            CString::new(bundle_path).context("invalid bundle_path for C string")?;

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
            _bundle_path_cstr: bundle_path_cstr,
            _options_min_block: options_min_block,
            _options_max_block: options_max_block,
            _options_sample_rate: options_sample_rate,
            _options_array: options_array,
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
