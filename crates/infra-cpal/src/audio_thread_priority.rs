//! Issue #670: promote the audio callback thread to a real-time
//! time-constraint policy on macOS.
//!
//! While the user plays, the Slint UI thread (render + spectrum FFT + meter
//! updates) saturates the cores. cpal's macOS data callback runs WITHOUT an
//! explicit real-time policy, so under that load the input callback is
//! preempted past its 64-frame deadline → xrun → the crackle. A reproduction
//! (engine `issue_670_beat_it_real_rig::live_thread_soup_reproduces_input_xruns`)
//! drives the real Beat It chains + a core-saturating UI proxy and measures
//! ~2% input overruns WITHOUT this policy and ~0% WITH it.
//!
//! Params that matter (learned the hard way):
//!   - `computation` is the per-period CPU the thread NEEDS — the chain's real
//!     cost, ~1/4 of the period, NOT most of it. Claiming ~90% makes several
//!     audio threads OVERSUBSCRIBE the realtime band and preempt each other,
//!     which is WORSE than plain scheduling.
//!   - `preemptible = 1` lets the paced audio threads cooperate.
//!
//! Called once per stream from the first callback invocation (the callback IS
//! the thread to promote). Windows / Linux are no-ops here (Linux uses the
//! JACK path's own scheduling).

/// Promote the CALLING thread to a real-time time-constraint policy sized to a
/// `period_ns` audio buffer. Returns true on success.
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
        // ns → mach absolute-time units: mach = ns * denom / numer.
        let ns_to_mach = |ns: u64| -> u32 {
            ((ns as u128 * tb.denom as u128) / tb.numer as u128).min(u32::MAX as u128) as u32
        };
        let period = ns_to_mach(period_ns);
        let mut policy = thread_time_constraint_policy_data_t {
            period,
            // The chain's real per-buffer cost — about a quarter of the period.
            // Modest on purpose: see the module note on oversubscription.
            computation: ns_to_mach(period_ns / 4),
            constraint: period,
            // Cooperative — paced audio threads share the realtime band.
            preemptible: 1,
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
    false
}
