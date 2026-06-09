//! Issue #670 — the audio callback thread must run with a real-time
//! time-constraint policy. On an M4 Pro the probe showed the audio thread
//! going OFF-CPU (cpu << wall) for milliseconds — impossible on that
//! hardware unless the thread is NOT real-time and gets preempted by the
//! GUI / other threads. cpal's macOS backend does not set a time-constraint
//! policy, so we promote the callback thread ourselves.
//!
//! This pins that the promotion call succeeds (sets the policy) on macOS.

#[cfg(target_os = "macos")]
#[test]
fn promote_current_thread_realtime_succeeds_on_macos() {
    // A 64-frame buffer @ 48 kHz ≈ 1.33 ms period.
    let ok = infra_cpal::promote_current_thread_realtime(1_333_333);
    assert!(
        ok,
        "BUG #670: failed to set a real-time time-constraint policy on the \
         audio thread — without it the thread is preempted off-CPU at small \
         buffers even on fast hardware (the buffer-64 crackle)."
    );
}
