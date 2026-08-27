//! Responsibility: runs a plugin's work() off the audio thread.
//!
//! Issue #670. The LV2 Worker extension exists to move non-realtime work
//! (allocation, I/O, heavy computation) OFF the audio thread:
//!
//!   - `schedule_work` (audio thread): copy the job into a lock-free SPSC
//!     ring and unpark the worker. RT-safe — no `work()`, no alloc, no lock.
//!   - worker thread: pop jobs, call `work()`; `work()`'s `respond` pushes
//!     the result into a second ring.
//!   - `Lv2Plugin::run` (audio thread): drain the response ring, call
//!     `work_response` + `end_run`, THEN the plugin's `run`.
//!
//! The previous implementation ran `work()` INLINE inside `schedule_work` —
//! on the audio thread — so a worker-using plugin (reverb, pitch shifter…)
//! did its heavy/allocating work on the realtime callback, stalling it
//! off-CPU (the buffer-64 crackle).

use crossbeam_queue::ArrayQueue;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;

use crate::host_abi::LV2Handle;

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
pub(crate) struct LV2WorkerSchedule {
    pub(crate) handle: *mut c_void,
    pub(crate) schedule_work:
        Option<unsafe extern "C" fn(handle: *mut c_void, size: u32, data: *const c_void) -> i32>,
}

#[repr(C)]
pub(crate) struct LV2WorkerInterface {
    pub(crate) work: Option<
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
    pub(crate) work_response:
        Option<unsafe extern "C" fn(instance: LV2Handle, size: u32, body: *const c_void) -> i32>,
    pub(crate) end_run: Option<unsafe extern "C" fn(instance: LV2Handle) -> i32>,
}

/// Holds the response ring; its pointer is passed to `work()` as the
/// `respond_handle`, so `worker_respond_callback` can find the ring.
struct Responder {
    response: Arc<ArrayQueue<WorkerMsg>>,
}

/// Owned by `Lv2Plugin`; the audio thread reaches `schedule` + `worker`
/// (unpark) through the pointer stored in `LV2WorkerSchedule.handle`.
pub(crate) struct WorkerState {
    handle: LV2Handle,
    worker_interface: *const LV2WorkerInterface,
    schedule: Arc<ArrayQueue<WorkerMsg>>,
    response: Arc<ArrayQueue<WorkerMsg>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    worker_thread: Option<std::thread::Thread>,
}

impl WorkerState {
    pub(crate) fn new(handle: LV2Handle, worker_interface: *const LV2WorkerInterface) -> Box<Self> {
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
    pub(crate) fn deliver_responses(&self) {
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
pub(crate) unsafe extern "C" fn worker_schedule_callback(
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
