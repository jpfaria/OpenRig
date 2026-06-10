//! Issue #670 — audio-thread load / xrun accounting for one
//! [`ChainRuntimeState`].
//!
//! The output callback (cpal `build_output_stream` / JACK process) is the
//! deadline-critical site: it pulls every block, runs the DSP, mixes and
//! limits, all inside one buffer period. When that work exceeds the
//! period the callback misses its deadline (an xrun) and the user hears a
//! dropout as crackle. These methods let the callback record its own
//! wall-clock cost so the overload is COUNTED and SURFACED instead of
//! crackling silently (the #670 symptom).
//!
//! RT-safety: `record_callback_load` does integer math and two `Relaxed`
//! atomic stores only — no allocation, no lock, no syscall. It is the
//! one new per-callback cost added on the audio thread; two integer
//! atomics against a buffer period of ~1.3 ms (64 frames @ 48 kHz) are
//! negligible and never themselves cause an xrun (CLAUDE.md invariant 8).

use std::sync::atomic::Ordering;

use crate::runtime_state::ChainRuntimeState;

/// Fixed-point scale for `peak_load_ppm`: parts-per-million of the buffer
/// period. 1_000_000 ppm == exactly at the deadline (load 1.0).
pub(crate) const LOAD_PPM_SCALE: u64 = 1_000_000;

impl ChainRuntimeState {
    /// Record one output callback's wall-clock cost (`elapsed_ns`) against
    /// its buffer deadline (`period_ns`). Called once per callback from
    /// the cpal / JACK output handler. RT-safe.
    ///
    /// - An overrun (`elapsed_ns > period_ns`) increments `xrun_count`.
    /// - `peak_load_ppm` keeps the worst `elapsed/period` ratio seen since
    ///   the last [`reset_load_stats`], in parts-per-million, for the UI
    ///   load meter.
    ///
    /// A non-positive `period_ns` (unknown sample rate / zero frames) is
    /// ignored — there is no deadline to miss.
    pub fn record_callback_load(&self, elapsed_ns: u64, period_ns: u64) {
        if period_ns == 0 {
            return;
        }
        if elapsed_ns > period_ns {
            self.xrun_count.fetch_add(1, Ordering::Relaxed);
        }
        let load_ppm = ((elapsed_ns as u128 * LOAD_PPM_SCALE as u128) / period_ns as u128) as u64;
        self.peak_load_ppm.fetch_max(load_ppm, Ordering::Relaxed);
    }

    /// Record one DSP-worker buffer's cost (issue #670). Updates the load
    /// meter ONLY — a late worker buffer is NOT an xrun by itself: unlike the
    /// old inline-callback design (where exceeding the HAL cycle made
    /// CoreAudio drop input — real damage), the worker's ring and the elastic
    /// buffer absorb a late buffer that catches up. Real damage is counted
    /// where it actually happens: an elastic underrun (output starved,
    /// `underrun_count`) or a dropped ring buffer ([`Self::record_dropped_buffer`]).
    /// The strict per-buffer lateness remains visible via
    /// [`Self::peak_callback_load`].
    pub fn record_worker_load(&self, elapsed_ns: u64, period_ns: u64) {
        if period_ns == 0 {
            return;
        }
        let load_ppm = ((elapsed_ns as u128 * LOAD_PPM_SCALE as u128) / period_ns as u128) as u64;
        self.peak_load_ppm.fetch_max(load_ppm, Ordering::Relaxed);
    }

    /// Count one input buffer DROPPED before processing (issue #670: the
    /// dsp-worker ring overflowed — the worker stalled for >16 buffers). This
    /// IS audible damage (a gap in the played signal), so it counts as an
    /// xrun. RT-safe: one atomic increment.
    pub fn record_dropped_buffer(&self) {
        self.xrun_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Total deadline overruns since the last [`reset_load_stats`]. Read by
    /// the GUI meter timer and by `QueryKind` for MCP / gRPC parity.
    pub fn xrun_count(&self) -> u64 {
        self.xrun_count.load(Ordering::Relaxed)
    }

    /// Worst callback load since the last [`reset_load_stats`], as a
    /// fraction of the buffer period (1.0 == exactly at the deadline,
    /// > 1.0 == overran). Reading does not reset it; the GUI polls.
    pub fn peak_callback_load(&self) -> f32 {
        self.peak_load_ppm.load(Ordering::Relaxed) as f32 / LOAD_PPM_SCALE as f32
    }

    /// Total output-side underruns across this chain's elastic buffers
    /// (issue #670). An underrun is an empty `pop` on the output callback:
    /// the input/DSP producer hasn't delivered the frame in time, so a
    /// silent gap (the click) is emitted. Distinct from an xrun (a slow
    /// callback): a light single chain crackling at buffer 64 with many
    /// underruns but ~zero xruns is starving the elastic buffer, not the
    /// CPU. Read off the audio thread.
    pub fn underrun_count(&self) -> u64 {
        self.output_routes
            .load()
            .iter()
            .map(|route| route.buffer.underrun_count())
            .sum()
    }

    /// Clear the xrun count and peak load. Called off the audio thread
    /// (e.g. when the user opens the audio diagnostics, or on a chain
    /// rebuild) so the meter reflects the current rig, not stale history.
    pub fn reset_load_stats(&self) {
        self.xrun_count.store(0, Ordering::Relaxed);
        self.peak_load_ppm.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "runtime_load_tests.rs"]
mod runtime_load_tests;
