//! Issue #670 — RED-first: the LV2 worker must run a plugin's `work()` on a
//! SEPARATE thread, not inline on the audio (calling) thread.
//!
//! The LV2 Worker extension exists precisely to move non-realtime work
//! (allocation, I/O, heavy computation) OFF the audio thread: `run()` calls
//! `schedule_work()`, which must QUEUE the job for a worker thread; the
//! result comes back on the next `run()` via `work_response()`. Our host
//! (`host.rs`) runs `work()` INLINE in `schedule_work` — so a plugin
//! (reverb, autotune, harmonizer…) that schedules work does that heavy work
//! ON the audio thread, stalling the callback off-CPU. That is the buffer-64
//! crackle on the user's LV2-heavy chains.
//!
//! This test schedules a job whose `work()` records the thread it ran on,
//! and asserts it ran on a DIFFERENT thread than the caller. RED today
//! (inline → same thread), GREEN once the worker is asynchronous.

#[test]
fn lv2_worker_runs_work_off_the_calling_thread() {
    let outcome = lv2::issue670_schedule_work_thread_check();
    assert!(
        !outcome.ran_inline,
        "BUG #670: the LV2 worker ran work() INLINE on the calling (audio) \
         thread — the plugin's non-realtime work executes on the audio \
         callback, stalling it. It must run on a dedicated worker thread \
         (schedule_work queues; the worker thread runs work(); the response \
         is delivered on the next run())."
    );
}
