//! Output level meter for chain UI — issue #496 / #32.
//!
//! Drains the L+R rings of a `StreamTap` (post-FX, pre-mixdown) and
//! returns the current peak as dBFS so the GUI can show
//! green/yellow/red without touching DSP. Lock-free on the audio
//! thread side (the rings are SPSC); the polling side runs from the
//! GUI thread on a timer.
//!
//! Colour buckets (Slint side decides the exact colour mapping, this
//! module only reports a single number):
//!
//! * `> 0 dBFS`  ⇒ clipping — red
//! * `-6 .. 0`   ⇒ hot, near-Spotify reference — green
//! * `-18 .. -6` ⇒ ok / a bit low — yellow shading
//! * `< -18`     ⇒ too quiet — orange
//! * `< -60`     ⇒ effectively silent — grey

use std::sync::Arc;

use crate::spsc::SpscRing;

/// dBFS value reported when the rings produce no audible sample.
pub const SILENT_DBFS: f32 = -120.0;

/// Drain all currently-queued samples across all channel rings of a
/// tap, and return the peak (max |sample|) of this window as dBFS.
/// Returns [`SILENT_DBFS`] when every ring is empty.
///
/// Works for both [`crate::stream_tap::StreamTap`] (always 2 rings,
/// L+R post-FX) and [`crate::input_tap::InputTap`] (N channels of raw
/// input). The caller is expected to invoke this periodically from a
/// GUI timer (e.g. 30 Hz). Each invocation reports the peak observed
/// since the previous call.
pub fn pop_peak_dbfs(rings: &[Arc<SpscRing<f32>>]) -> f32 {
    let mut peak: f32 = 0.0;
    let mut saw_any = false;
    for ring in rings {
        while let Some(s) = ring.pop() {
            saw_any = true;
            let a = s.abs();
            if a > peak {
                peak = a;
            }
        }
    }
    if !saw_any {
        return SILENT_DBFS;
    }
    20.0 * peak.max(1e-12).log10()
}

#[cfg(test)]
#[path = "output_meter_tests.rs"]
mod tests;
