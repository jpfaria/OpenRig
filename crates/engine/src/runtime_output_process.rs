//! Output-side audio callback: what one physical output device is handed on
//! every callback (issue #127 split from `runtime.rs`, which keeps the input
//! side).
//!
//! Both entry points run on the audio thread and honour the same contract as
//! the rest of `engine`: ZERO allocation, ZERO locking, ZERO I/O. Routes are
//! read through `ArcSwap` and frames through per-route SPSC rings, so a live
//! edit on another thread never blocks the callback.
//!
//! Re-exported from `crate::runtime`, so the public paths
//! `engine::runtime::process_output_f32{,_mixed}` are unchanged.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::runtime_dsp::{ensure_flush_to_zero, output_limiter};
use crate::runtime_io::write_output_frame;
use crate::runtime_probe::{PROBE_DETECT_THRESHOLD, PROBE_FIRED, PROBE_IDLE};
use crate::runtime_state::ChainRuntimeState;

pub fn process_output_f32(
    runtime: &Arc<ChainRuntimeState>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
) {
    if runtime.is_draining() {
        out.fill(0.0);
        return;
    }
    ensure_flush_to_zero();

    // Snapshot the current routes via ArcSwap — no lock on the RT thread.
    let routes = runtime.output_routes.load();
    let route = match routes.get(output_index) {
        Some(r) => r,
        None => {
            out.fill(0.0);
            return;
        }
    };
    // Issue #440 / #350 fidelity: apply Chain.volume to the AudioFrame
    // BEFORE `write_output_frame` (which runs the output limiter). Applying
    // it after the limiter let a hot chain × volume>100 clip the DAC with
    // nothing to catch it on the single-stream path. Single atomic load of
    // volume_pct per callback. No clamp here — the limiter inside
    // write_output_frame is the gate (this file's pinned contract:
    // "clipping is the output limiter's job"). Sub-knee signals are
    // unaffected (tanh transparent below 0.95), so k01–k04 stay green.
    let volume_ratio = runtime.volume_pct() / 100.0;
    let num_frames = out.len() / output_total_channels;
    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let mut processed = route.buffer.pop();
        if volume_ratio != 1.0 {
            processed = processed.scaled(volume_ratio);
        }
        write_output_frame(
            processed,
            &route.output_channels,
            frame,
            route.output_mixdown,
        );
    }

    // Output mute: silence the entire output stage when toggled by any
    // consumer (e.g. the Tuner window). Single atomic load — cheap.
    if runtime
        .output_muted
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        out.fill(0.0);
    }

    // Latency probe detection: only the primary output (index 0) scans.
    // When Fired, look for the leading edge of the injected beep. Measure
    // wall-clock nanos from injection to detection; that is the real
    // end-to-end latency of the signal path for the user.
    if output_index == 0 && runtime.probe_state.load(Ordering::Acquire) == PROBE_FIRED {
        let detected_at_idx = out.iter().position(|s| s.abs() > PROBE_DETECT_THRESHOLD);
        if detected_at_idx.is_some() {
            let now = runtime.created_at.elapsed().as_nanos() as u64;
            let injected_at = runtime.last_input_nanos.load(Ordering::Relaxed);
            // Measure wall-clock nanos from the input callback that
            // injected the beep to this output callback that detected
            // it. This is callback-level granularity; we intentionally
            // do NOT add the intra-buffer offset because that couples
            // the measurement to signal amplitude (through the
            // threshold-crossing position) and inflates readings for
            // chains that attenuate the signal.
            let delta = now.saturating_sub(injected_at);
            runtime
                .measured_latency_nanos
                .store(delta, Ordering::Relaxed);
            runtime.probe_state.store(PROBE_IDLE, Ordering::Release);
        }
    }
}

/// Drive one physical output device from the N per-input runtimes of a
/// chain (issue #350, phase 3). Each `InputBlock` entry on a distinct
/// physical device is its own isolated [`ChainRuntimeState`] with its own
/// SPSC output ring; the shared output device must sum them at the
/// backend — the ONLY place CLAUDE.md invariant #4 permits mixing across
/// streams. Each ring still has exactly one producer (its own input
/// callback) and is consumed once here, so SPSC is preserved.
///
/// Single-runtime chains (the 99% case, and every `volume_invariants` /
/// golden scenario) take the `[1]` fast path: `process_output_f32` writes
/// straight into `out` with ZERO extra work — byte-identical to pre-#350.
///
/// Multi-runtime: each runtime's output (already per-runtime limited +
/// volume-scaled inside `process_output_f32`) is rendered into the
/// caller-owned `scratch` and summed into `out`; the summed buffer then
/// passes through `output_limiter` (the same tanh the chain already
/// trusts to hold a multi-stream sum transparent below 0 dBFS — see the
/// route-mix note in `mix_segment_into_routes`) so the device never
/// receives a clipped buffer. `scratch` MUST be pre-allocated by the
/// caller at stream-build time and be at least `out.len()` long — this
/// function performs ZERO allocation and ZERO locking on the audio
/// thread (it only does the lock-free work `process_output_f32` already
/// did, once per runtime).
pub fn process_output_f32_mixed(
    runtimes: &[Arc<ChainRuntimeState>],
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    match runtimes {
        [] => out.fill(0.0),
        // Fast path: one isolated stream → byte-identical to pre-#350.
        [runtime] => process_output_f32(runtime, output_index, out, output_total_channels),
        many => {
            out.fill(0.0);
            let n = out.len();
            for runtime in many {
                let buf = &mut scratch[..n];
                process_output_f32(runtime, output_index, buf, output_total_channels);
                for (dst, src) in out.iter_mut().zip(buf.iter()) {
                    *dst += *src;
                }
            }
            // Backend mix saturation guard: N per-runtime-limited streams
            // can sum past 1.0; tanh holds it transparent below 0 dBFS.
            for s in out.iter_mut() {
                *s = output_limiter(*s);
            }
        }
    }
}
