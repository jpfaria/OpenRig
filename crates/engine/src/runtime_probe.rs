//! Latency probe state machine + impl methods on `ChainRuntimeState`.
//!
//! Lifted out of `runtime.rs` (slice 6 of the Phase 2 split) so the parent
//! file gets back under the 600 LOC cap.
//!
//! What lives here:
//!   - PROBE_* constants — the state machine (Idle / Armed / Fired) plus
//!     the beep waveform parameters (frames, frequency) and the output
//!     amplitude threshold that counts as "the probe arrived".
//!   - The four impl methods on `ChainRuntimeState` that drive the probe
//!     from the UI thread (`arm_latency_probe`, `cancel_latency_probe`,
//!     `probe_in_flight`, `measured_latency_ms`).
//!
//! What's NOT here: the audio-thread side of the probe (beep injection in
//! `process_input_f32`, beep detection in `process_output_f32`) — that
//! stays in `runtime.rs` next to the rest of the audio callback hot path.
//! This module only owns the constants and the UI-thread control surface;
//! it adds zero work to the audio thread.
//!
//! Visibility: PROBE_* constants are `pub(crate)` so the audio-thread side
//! in `runtime.rs` and the `probe.rs` sibling (which generates the same
//! beep for offline latency measurements) keep working unchanged via the
//! existing `crate::runtime::PROBE_*` re-exports.

use std::sync::atomic::Ordering;

use crate::runtime_state::ChainRuntimeState;

pub(crate) const PROBE_IDLE: u8 = 0;
pub(crate) const PROBE_ARMED: u8 = 1;
pub(crate) const PROBE_FIRED: u8 = 2;

/// Number of audio frames the probe beep occupies. At 48 kHz this is
/// 128 / 48000 ≈ 2.7 ms of a short 1 kHz sine burst — audible as a "tick"
/// without being intrusive.
pub(crate) const PROBE_BEEP_FRAMES: usize = 128;
/// Frequency of the sine used for the probe beep, in Hz.
pub(crate) const PROBE_BEEP_FREQ: f32 = 1000.0;
/// Output sample amplitude that counts as "the probe arrived". Set low
/// enough to catch the beep even through an amp model that attenuates
/// or filters the 1 kHz sine, but well above a realistic digital noise
/// floor so background hum does not false-trigger detection.
pub(crate) const PROBE_DETECT_THRESHOLD: f32 = 0.05;

impl ChainRuntimeState {
    /// Arm the latency probe. The next input callback will inject a short
    /// beep into the signal path, and the first output callback to see it
    /// arrive at the output stage records the measured latency. Safe to
    /// call while audio is flowing; idempotent if already armed or fired.
    pub fn arm_latency_probe(&self) {
        self.probe_state.store(PROBE_ARMED, Ordering::Release);
    }

    pub fn probe_in_flight(&self) -> bool {
        self.probe_state.load(Ordering::Acquire) != PROBE_IDLE
    }

    /// Reset the probe state to Idle. Used by the UI when the display
    /// window expires so a probe that never completed (e.g. the beep was
    /// consumed by a chain rebuild, or the chain was disabled before the
    /// output callback ran) does not stay armed across sessions.
    pub fn cancel_latency_probe(&self) {
        self.probe_state.store(PROBE_IDLE, Ordering::Release);
    }

    pub fn measured_latency_ms(&self) -> f32 {
        let nanos = self.measured_latency_nanos.load(Ordering::Relaxed);
        nanos as f32 / 1_000_000.0
    }
}
