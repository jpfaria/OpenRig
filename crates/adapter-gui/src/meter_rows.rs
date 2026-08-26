//! Responsibility: writes the meter readings into the rows on screen.

use crate::meter_math::apply_chain_volume_db;
use crate::meter_taps::StreamMeterReading;

use engine::output_meter::SILENT_DBFS;

/// Build the per-chain `stream_meters` row payload the GUI must show.
///
/// Issue #532: the row length is owned by the project state — one
/// slot per input entry in the chain (with a min of 1 mirroring
/// `replace_project_chains`'s `.max(1)` clamp) — NOT by the engine's
/// transient per-tick stream count. If the engine reports more streams
/// than the project owns (transient mid-rebuild after a preset switch),
/// the extra readings are dropped. If it reports fewer (sibling chain
/// re-spawning after a toggle), the missing slots stay [`SILENT_DBFS`].
/// Both symptoms reported in #532 collapse to the same fix.
///
/// Issue #750: when `enabled` is false the row is EMPTY — the per-stream graph
/// is a live surface that must not show on a disabled chain. This overrides the
/// `.max(1)` clamp, so the timer can't re-grow the footer a tick after the
/// chain is switched off.
///
/// The OUTPUT reading is scaled by `apply_chain_volume_db` because the
/// stream_tap reads the signal BEFORE the audio callback applies the
/// chain volume slider (#496). INPUT is untouched.
pub fn rebuild_stream_meters_row(
    engine_readings: &[StreamMeterReading],
    project_input_count: usize,
    chain_volume: f32,
    enabled: bool,
) -> Vec<crate::StreamMeter> {
    // #750: the per-stream graph is a LIVE surface — a disabled chain renders
    // no rows at all, overriding the `.max(1)` clamp below.
    if !enabled {
        return Vec::new();
    }
    let len = project_input_count.max(1);
    (0..len)
        .map(|i| match engine_readings.get(i) {
            Some(r) => crate::StreamMeter {
                in_dbfs: r.in_dbfs,
                out_dbfs: apply_chain_volume_db(r.out_dbfs, chain_volume),
            },
            None => crate::StreamMeter {
                in_dbfs: SILENT_DBFS,
                out_dbfs: SILENT_DBFS,
            },
        })
        .collect()
}
