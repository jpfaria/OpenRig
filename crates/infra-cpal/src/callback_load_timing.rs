//! Issue #670 — audio-callback deadline timing seam.
//!
//! In OpenRig the heavy block DSP (`processor.process_buffer` — NAM
//! inference, IR convolution, etc.) runs inside `process_input_f32`, on
//! the cpal INPUT callback and, on Linux/JACK, on the offload worker. The
//! output side only pops the elastic buffer and applies volume + limiter,
//! so the buffer-64 overload the user hears as crackle is an
//! INPUT-callback deadline miss.
//!
//! This module is the thin, testable seam between that callback and the
//! engine's per-chain xrun counter
//! ([`engine::runtime::ChainRuntimeState::record_callback_load`]): the
//! callback records its own wall-clock cost after processing one buffer,
//! so the overload is counted (and surfaced) instead of crackling
//! silently.
//!
//! RT-safety: `record_callback_deadline` is integer math + one atomic
//! pair, no allocation/lock/syscall — safe at the tail of the audio
//! callback (CLAUDE.md invariant 8).

use std::time::Duration;

use engine::runtime::ChainRuntimeState;

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// The buffer deadline in nanoseconds for `frames` at `sample_rate_hz`.
/// Returns 0 when either input is 0 — there is no deadline to measure
/// (unknown sample rate, or an empty callback buffer).
pub(crate) fn callback_period_ns(frames: usize, sample_rate_hz: u32) -> u64 {
    if frames == 0 || sample_rate_hz == 0 {
        return 0;
    }
    (frames as u64 * NANOS_PER_SEC) / sample_rate_hz as u64
}

/// Record one audio callback's wall-clock cost (`elapsed`) against its
/// buffer deadline on `runtime`. Called once per callback, after the
/// block DSP finishes. RT-safe.
pub(crate) fn record_callback_deadline(
    runtime: &ChainRuntimeState,
    elapsed: Duration,
    frames: usize,
    sample_rate_hz: u32,
) {
    let period_ns = callback_period_ns(frames, sample_rate_hz);
    if period_ns == 0 {
        return;
    }
    let elapsed_ns = elapsed.as_nanos().min(u64::MAX as u128) as u64;
    runtime.record_callback_load(elapsed_ns, period_ns);
}

/// Issue #670 probe: this thread's consumed CPU time in nanoseconds. Used
/// alongside the wall clock to tell an off-CPU stall (preemption / page
/// fault → wall ≫ cpu) from an on-CPU cost (compute / cache → wall ≈ cpu).
/// Returns 0 if the clock is unavailable.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn thread_cpu_time_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    if rc != 0 {
        return 0;
    }
    (ts.tv_sec as u64) * 1_000_000_000 + (ts.tv_nsec as u64)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) fn thread_cpu_time_ns() -> u64 {
    0
}

#[cfg(test)]
#[path = "callback_load_timing_tests.rs"]
mod callback_load_timing_tests;
