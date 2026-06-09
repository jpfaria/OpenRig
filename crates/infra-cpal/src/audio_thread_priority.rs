//! Issue #670: promote the audio callback thread to real-time scheduling.
//!
//! The probe showed the audio thread going OFF-CPU (cpu << wall) for
//! milliseconds at buffer 64 — on an M4 Pro, which is impossible from CPU
//! cost alone. The cause: cpal's macOS backend runs the data callback
//! without an explicit real-time time-constraint policy, so under GUI /
//! other-thread load the scheduler preempts it. The fix is the standard
//! pro-audio one: set a real-time time-constraint policy on the callback
//! thread, telling the kernel it needs `computation` time every `period`
//! and must not be preempted past `constraint`.
//!
//! Called once per stream from the first callback invocation (the callback
//! IS the thread we need to promote). Windows / Linux land here as no-ops
//! for now (TODO: AvSetMmThreadCharacteristics "Pro Audio" / SCHED_FIFO).

/// Promote the CALLING thread to a real-time time-constraint policy sized to
/// a `period_ns` audio buffer. Returns true on success.
#[cfg(target_os = "macos")]
pub fn promote_current_thread_realtime(period_ns: u64) -> bool {
    use mach2::mach_init::mach_thread_self;
    use mach2::mach_time::{mach_timebase_info, mach_timebase_info_data_t};
    use mach2::thread_policy::{
        thread_policy_set, thread_time_constraint_policy_data_t, THREAD_TIME_CONSTRAINT_POLICY,
        THREAD_TIME_CONSTRAINT_POLICY_COUNT,
    };

    unsafe {
        let mut tb = mach_timebase_info_data_t { numer: 0, denom: 0 };
        if mach_timebase_info(&mut tb) != 0 || tb.numer == 0 || tb.denom == 0 {
            return false;
        }
        // Convert nanoseconds to mach absolute-time units: mach = ns * denom / numer.
        let ns_to_mach = |ns: u64| -> u32 {
            ((ns as u128 * tb.denom as u128) / tb.numer as u128).min(u32::MAX as u128) as u32
        };
        let period = ns_to_mach(period_ns);
        let mut policy = thread_time_constraint_policy_data_t {
            period,
            // We need most of each period of guaranteed CPU.
            computation: ns_to_mach(period_ns * 9 / 10),
            // Hard deadline = one buffer period.
            constraint: period,
            // Not preemptible — audio must finish its buffer.
            preemptible: 0,
        };
        let kr = thread_policy_set(
            mach_thread_self(),
            THREAD_TIME_CONSTRAINT_POLICY,
            &mut policy as *mut _ as *mut libc::c_int,
            THREAD_TIME_CONSTRAINT_POLICY_COUNT,
        );
        kr == mach2::kern_return::KERN_SUCCESS
    }
}

#[cfg(not(target_os = "macos"))]
pub fn promote_current_thread_realtime(_period_ns: u64) -> bool {
    // TODO #670 follow-up: Windows AvSetMmThreadCharacteristics("Pro Audio")
    // + AvSetMmThreadPriority; Linux non-jack SCHED_FIFO.
    false
}
