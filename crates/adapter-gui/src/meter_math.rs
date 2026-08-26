//! Responsibility: turns raw peaks into the dBFS a meter shows.

use application::audio_taps::AudioTap;
use engine::output_meter::SILENT_DBFS;
use std::sync::Arc;

/// Apply the chain's volume slider (in percent, 100 = unity) to a
/// raw peak-dBFS reading. The stream_tap is captured BEFORE the
/// audio callback scales by `volume_pct/100`, so the OUTPUT meter
/// has to add `20·log10(volume_pct/100)` on the GUI side to reflect
/// what actually reaches the DAC. `SILENT_DBFS` round-trips
/// unchanged; a 0 % volume is treated as silence.
pub fn apply_chain_volume_db(base_dbfs: f32, volume_pct: f32) -> f32 {
    if base_dbfs <= SILENT_DBFS + 1.0 {
        return SILENT_DBFS;
    }
    if volume_pct <= 0.0 {
        return SILENT_DBFS;
    }
    base_dbfs + 20.0 * (volume_pct / 100.0).log10()
}

/// Poll one stream's input and output subscriptions and return
/// `(input_peak_dbfs, output_peak_dbfs)`. Either side reports
/// [`SILENT_DBFS`] when it has no subscription — a stream that is not
/// there to tap is silent, never a fabricated level.
///
/// Pure over the supplied taps — no Slint, no engine runtime,
/// directly testable. The reduction itself lives behind the seam
/// ([`AudioTap::poll_peak_dbfs`]), so a frontend that carries only
/// finished readings reports the same numbers.
pub fn compute_meter_for_chain(
    input: Option<&Arc<dyn AudioTap>>,
    output: Option<&Arc<dyn AudioTap>>,
) -> (f32, f32) {
    (
        input.map_or(SILENT_DBFS, |tap| tap.poll_peak_dbfs()),
        output.map_or(SILENT_DBFS, |tap| tap.poll_peak_dbfs()),
    )
}

/// Issue #670: a chain is "overloading" when its audio callback counted
/// MORE deadline misses (xruns) than the previous meter poll saw — i.e.
/// the user is hearing dropouts right now. The timer keeps the previous
/// per-chain count; a decrease means the counter was reset (e.g. on a
/// chain rebuild), not a fresh overrun.
pub(crate) fn chain_overloaded(prev_xruns: u64, cur_xruns: u64) -> bool {
    cur_xruns > prev_xruns
}
