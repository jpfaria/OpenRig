//! Responsibility: reports whether the LV2 worker ran work() inline.
//!
//! Issue #670 test seam. A correct async worker runs `work()` on its own
//! thread; the realtime violation this probe catches is `work()` running on
//! the thread that scheduled it — the audio callback.

use std::ffi::c_void;

use crate::host_abi::LV2Handle;
use crate::host_worker::{worker_schedule_callback, LV2WorkerInterface, WorkerState};

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
