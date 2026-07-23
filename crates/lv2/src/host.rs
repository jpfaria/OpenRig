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
// Constants
// ---------------------------------------------------------------------------

const LV2_URID_MAP_URI: &str = "http://lv2plug.in/ns/ext/urid#map";
const LV2_BUF_SIZE_BOUNDED_URI: &str = "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength";
const LV2_OPTIONS_URI: &str = "http://lv2plug.in/ns/ext/options#options";
const LV2_BUF_SIZE_MAX_URI: &str = "http://lv2plug.in/ns/ext/buf-size#maxBlockLength";
const LV2_BUF_SIZE_MIN_URI: &str = "http://lv2plug.in/ns/ext/buf-size#minBlockLength";
const LV2_ATOM_INT_URI: &str = "http://lv2plug.in/ns/ext/atom#Int";
const LV2_ATOM_FLOAT_URI: &str = "http://lv2plug.in/ns/ext/atom#Float";
const LV2_PARAM_SAMPLE_RATE_URI: &str = "http://lv2plug.in/ns/ext/parameters#sampleRate";
const LV2_WORKER_SCHEDULE_URI: &str = "http://lv2plug.in/ns/ext/worker#schedule";
const LV2_WORKER_INTERFACE_URI: &str = "http://lv2plug.in/ns/ext/worker#interface";

// ---------------------------------------------------------------------------
// URID map
// ---------------------------------------------------------------------------

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
    let uri_str = unsafe { CStr::from_ptr(uri) }.to_str().unwrap_or("");
    map.map(uri_str)
}

// ---------------------------------------------------------------------------
// LV2 Options
// ---------------------------------------------------------------------------

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
// LV2 Worker (ASYNC — issue #670)
//
// The LV2 Worker extension exists to move non-realtime work (allocation, I/O,
// heavy computation) OFF the audio thread. `run()` calls `schedule_work`,
// which must QUEUE the job for a dedicated worker thread; the worker runs
// `work()`; its response comes back on the NEXT `run()` via `work_response`.
//
// The previous implementation ran `work()` INLINE inside `schedule_work` —
// on the audio thread — so a worker-using plugin (reverb, pitch shifter…)
// did its heavy/allocating work on the realtime callback, stalling it
// off-CPU (the buffer-64 crackle). This version is asynchronous:
//   - `schedule_work` (audio thread): copy the job into a lock-free SPSC
//     ring and unpark the worker. RT-safe — no `work()`, no alloc, no lock.
//   - worker thread: pop jobs, call `work()`; `work()`'s `respond` pushes the
//     result into a second ring.
//   - `Lv2Plugin::run` (audio thread): drain the response ring, call
//     `work_response` + `end_run`, THEN the plugin's `run`.
// ---------------------------------------------------------------------------

use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;

/// Max bytes of a single worker job/response. LV2 worker payloads are small
/// command structs; 8 KiB is generous and keeps the ring slots stack-sized
/// (no per-push allocation on the audio thread).
const WORKER_MSG_MAX: usize = 8192;
/// Ring depth (jobs in flight). Drop-on-full — a flooded worker is a
/// misbehaving plugin, and dropping a job is less harmful than blocking the
/// audio thread.
const WORKER_RING_CAP: usize = 64;

struct WorkerMsg {
    len: u32,
    data: [u8; WORKER_MSG_MAX],
}

impl WorkerMsg {
    fn from_raw(size: u32, data: *const c_void) -> Self {
        let mut buf = [0u8; WORKER_MSG_MAX];
        let n = (size as usize).min(WORKER_MSG_MAX);
        if !data.is_null() && n > 0 {
            unsafe { std::ptr::copy_nonoverlapping(data as *const u8, buf.as_mut_ptr(), n) };
        }
        WorkerMsg {
            len: n as u32,
            data: buf,
        }
    }
}

/// Raw FFI pointer wrapper so the plugin handle / fn pointers can be moved
/// into the worker thread. SAFETY: the LV2 Worker contract explicitly allows
/// `work()` to run concurrently with `run()`; the plugin owns that safety.
struct SendPtr<T>(T);
unsafe impl<T> Send for SendPtr<T> {}

#[repr(C)]
struct LV2WorkerSchedule {
    handle: *mut c_void,
    schedule_work:
        Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> i32>,
}

#[repr(C)]
struct LV2WorkerInterface {
    work: Option<
        unsafe extern "C" fn(
            instance: LV2Handle,
            respond: Option<
                unsafe extern "C" fn(handle: LV2Handle, size: u32, data: *const c_void) -> i32,
            >,
            respond_handle: LV2Handle,
            size: u32,
            data: *const c_void,
        ) -> i32,
    >,
    work_response:
        Option<unsafe extern "C" fn(instance: LV2Handle, size: u32, body: *const c_void) -> i32>,
    end_run: Option<unsafe extern "C" fn(instance: LV2Handle) -> i32>,
}

/// Holds the response ring; its pointer is passed to `work()` as the
/// `respond_handle`, so `worker_respond_callback` can find the ring.
struct Responder {
    response: Arc<ArrayQueue<WorkerMsg>>,
}

/// Owned by `Lv2Plugin`; the audio thread reaches `schedule` + `worker`
/// (unpark) through the pointer stored in `LV2WorkerSchedule.handle`.
struct WorkerState {
    handle: LV2Handle,
    worker_interface: *const LV2WorkerInterface,
    schedule: Arc<ArrayQueue<WorkerMsg>>,
    response: Arc<ArrayQueue<WorkerMsg>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    worker_thread: Option<std::thread::Thread>,
}

impl WorkerState {
    fn new(handle: LV2Handle, worker_interface: *const LV2WorkerInterface) -> Box<Self> {
        let schedule: Arc<ArrayQueue<WorkerMsg>> = Arc::new(ArrayQueue::new(WORKER_RING_CAP));
        let response: Arc<ArrayQueue<WorkerMsg>> = Arc::new(ArrayQueue::new(WORKER_RING_CAP));
        let stop = Arc::new(AtomicBool::new(false));

        let w_schedule = Arc::clone(&schedule);
        let w_response = Arc::clone(&response);
        let w_stop = Arc::clone(&stop);
        let w_handle = SendPtr(handle);
        let w_iface = SendPtr(worker_interface);

        let worker = std::thread::Builder::new()
            .name("lv2-worker".into())
            .spawn(move || {
                let _ = &w_handle;
                let _ = &w_iface;
                let responder = Responder {
                    response: w_response,
                };
                let work_fn = unsafe { (*w_iface.0).work };
                loop {
                    if w_stop.load(AtomicOrdering::Acquire) {
                        break;
                    }
                    let mut did_work = false;
                    while let Some(msg) = w_schedule.pop() {
                        did_work = true;
                        if let Some(work) = work_fn {
                            unsafe {
                                work(
                                    w_handle.0,
                                    Some(worker_respond_callback),
                                    &responder as *const Responder as LV2Handle,
                                    msg.len,
                                    msg.data.as_ptr() as *const c_void,
                                );
                            }
                        }
                    }
                    if !did_work {
                        std::thread::park();
                    }
                }
            })
            .ok();
        let worker_thread = worker.as_ref().map(|h| h.thread().clone());

        Box::new(WorkerState {
            handle,
            worker_interface,
            schedule,
            response,
            stop,
            worker,
            worker_thread,
        })
    }

    /// Drain responses on the audio thread (in `run`) and deliver them to the
    /// plugin via `work_response`, then `end_run`. RT-safe: ring pops + FFI.
    fn deliver_responses(&self) {
        let iface = unsafe { &*self.worker_interface };
        let mut any = false;
        while let Some(msg) = self.response.pop() {
            any = true;
            if let Some(work_response) = iface.work_response {
                unsafe {
                    work_response(self.handle, msg.len, msg.data.as_ptr() as *const c_void);
                }
            }
        }
        if any {
            if let Some(end_run) = iface.end_run {
                unsafe { end_run(self.handle) };
            }
        }
    }
}

impl Drop for WorkerState {
    fn drop(&mut self) {
        self.stop.store(true, AtomicOrdering::Release);
        if let Some(t) = &self.worker_thread {
            t.unpark();
        }
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

/// schedule_work — audio thread. Queue the job, wake the worker, return.
/// Never runs `work()` inline (the #670 fix).
unsafe extern "C" fn worker_schedule_callback(
    ws_handle: *mut c_void,
    size: u32,
    data: *const c_void,
) -> i32 {
    if ws_handle.is_null() {
        return 0;
    }
    let state = unsafe { &*(ws_handle as *const WorkerState) };
    // Drop-on-full: a flooded worker is a plugin bug; never block audio.
    let _ = state.schedule.push(WorkerMsg::from_raw(size, data));
    if let Some(t) = &state.worker_thread {
        t.unpark();
    }
    0
}

/// respond — worker thread. Queue the response for delivery on the next run().
unsafe extern "C" fn worker_respond_callback(
    respond_handle: LV2Handle,
    size: u32,
    data: *const c_void,
) -> i32 {
    if respond_handle.is_null() {
        return 0;
    }
    let responder = unsafe { &*(respond_handle as *const Responder) };
    let _ = responder.response.push(WorkerMsg::from_raw(size, data));
    0
}

// ---------------------------------------------------------------------------
// Port metadata
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
// Lv2Plugin
// ---------------------------------------------------------------------------

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

// ── Issue #670 test seam: does the LV2 worker run work() inline? ──────────
// The LV2 Worker extension must run a plugin's `work()` on a SEPARATE thread
// (schedule_work queues; the worker thread runs work(); the response is
// delivered on the next run()). If `work()` runs INLINE on the audio thread,
// the plugin's non-realtime work stalls the callback (the #670 buffer-64
// crackle on LV2-heavy chains). This seam schedules a job whose work()
// records its thread and reports whether it ran on the calling thread.

/// Result of [`issue670_schedule_work_thread_check`].
pub struct WorkerThreadCheck {
    /// True when `work()` ran on the SAME thread that scheduled it (inline)
    /// — the realtime-violating behaviour #670 is about.
    pub ran_inline: bool,
}

static ISSUE670_WORKER_THREAD: std::sync::Mutex<Option<std::thread::ThreadId>> =
    std::sync::Mutex::new(None);

unsafe extern "C" fn issue670_recording_work(
    _instance: LV2Handle,
    _respond: Option<unsafe extern "C" fn(LV2Handle, u32, *const c_void) -> i32>,
    _respond_handle: LV2Handle,
    _size: u32,
    _data: *const c_void,
) -> i32 {
    *ISSUE670_WORKER_THREAD.lock().unwrap() = Some(std::thread::current().id());
    0
}

/// Schedule one worker job whose `work()` records its thread, and report
/// whether it ran inline on the calling thread. Used by the #670 worker test.
pub fn issue670_schedule_work_thread_check() -> WorkerThreadCheck {
    *ISSUE670_WORKER_THREAD.lock().unwrap() = None;
    // Box the interface so it outlives the worker thread (which reads it).
    let iface = Box::new(LV2WorkerInterface {
        work: Some(issue670_recording_work),
        work_response: None,
        end_run: None,
    });
    let mut state = WorkerState::new(std::ptr::null_mut(), &*iface as *const LV2WorkerInterface);
    let calling = std::thread::current().id();
    unsafe {
        worker_schedule_callback(
            state.as_mut() as *mut WorkerState as *mut c_void,
            0,
            std::ptr::null(),
        );
    }
    // A correct async worker runs work() on its own thread — wait briefly for
    // it. The current inline implementation has already run it synchronously.
    for _ in 0..500 {
        if ISSUE670_WORKER_THREAD.lock().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let ran_on = *ISSUE670_WORKER_THREAD.lock().unwrap();
    WorkerThreadCheck {
        ran_inline: ran_on == Some(calling),
    }
}
