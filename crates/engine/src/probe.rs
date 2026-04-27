//! Latency probe — measures chain DSP time on a temporary, isolated
//! runtime instance.
//!
//! The probe builds a fresh [`ChainRuntimeState`] for the requested chain,
//! injects a single 1 kHz sine pulse, runs it through the chain via
//! [`process_input_f32`], times the call, and drops the runtime. Live
//! audio is never touched — the probe does not share a stream with the
//! audio interface, so there is no detector to fool, no buffer-state
//! dependency, and no competition with whatever signal happens to be
//! flowing in or out at probe time.
//!
//! Returned value is **chain DSP duration** in milliseconds for one
//! 256-frame buffer. Toggling a block in or out of the chain changes the
//! reading directly: a disabled block becomes a `Bypass` runtime node
//! (no processing), so its contribution to the measurement disappears.
//!
//! Lives in its own module — does not bloat `runtime.rs` (issue #276).
//! See issue #334 for the full design discussion.
//!
//! [`ChainRuntimeState`]: crate::runtime::ChainRuntimeState

use crate::runtime::{
    build_chain_runtime_state, process_input_f32, DEFAULT_ELASTIC_TARGET, PROBE_BEEP_FRAMES,
    PROBE_BEEP_FREQ,
};
use project::chain::Chain;
use std::sync::Arc;

/// Number of frames in the probe pulse buffer. One callback's worth at
/// 48 kHz is enough to exercise every block in the chain at least once.
const PROBE_BUFFER_FRAMES: usize = 256;

/// Channel count for the probe input. Mono is sufficient — every chain
/// has a primary input on channel 0, and the chain's own routing handles
/// the mono → stereo upsample if any block needs stereo.
const PROBE_BUFFER_CHANNELS: usize = 1;

/// Build a temporary runtime for `chain`, run a 1 kHz pulse through it,
/// and return the elapsed DSP time in milliseconds.
///
/// Returns `0.0` on any build error (the UI treats zero as "not measured"
/// and hides the badge).
pub fn measure_chain_dsp_latency_ms(chain: &Chain, sample_rate: f32) -> f32 {
    let runtime = match build_chain_runtime_state(chain, sample_rate, &[DEFAULT_ELASTIC_TARGET]) {
        Ok(rt) => Arc::new(rt),
        Err(_) => return 0.0,
    };

    let mut data = vec![0.0_f32; PROBE_BUFFER_FRAMES * PROBE_BUFFER_CHANNELS];
    let beep_frames = PROBE_BEEP_FRAMES.min(PROBE_BUFFER_FRAMES);
    let nominal_sr = 48_000.0_f32;
    for f in 0..beep_frames {
        let t = f as f32 / nominal_sr;
        let envelope = (std::f32::consts::PI * f as f32 / beep_frames as f32).sin();
        let sample = (2.0 * std::f32::consts::PI * PROBE_BEEP_FREQ * t).sin() * 0.95 * envelope;
        for ch in 0..PROBE_BUFFER_CHANNELS {
            data[f * PROBE_BUFFER_CHANNELS + ch] = sample;
        }
    }

    let start = std::time::Instant::now();
    process_input_f32(&runtime, 0, &data, PROBE_BUFFER_CHANNELS);
    let elapsed = start.elapsed();

    elapsed.as_nanos() as f32 / 1_000_000.0
}
