//! #771: the DI meter row reads the DI stream's OWN levels — the linear
//! peaks the output callback's playback mix maintains — never a mirror of
//! the chain's meters (the DI no longer rides the chain path).

use engine::output_meter::SILENT_DBFS;

/// Convert the DI playback's linear `(in, out)` peaks into the meter-row
/// dBFS pair. Not playing, still rendering (`None`), or a zero peak all
/// read silent.
pub fn di_meter_from_peaks(peaks: Option<(f32, f32)>, playing: bool) -> crate::StreamMeter {
    let (in_peak, out_peak) = match (playing, peaks) {
        (true, Some(p)) => p,
        _ => (0.0, 0.0),
    };
    crate::StreamMeter {
        in_dbfs: linear_to_dbfs(in_peak),
        out_dbfs: linear_to_dbfs(out_peak),
    }
}

fn linear_to_dbfs(peak: f32) -> f32 {
    if peak <= 0.0 {
        return SILENT_DBFS;
    }
    (20.0 * peak.log10()).max(SILENT_DBFS)
}

#[cfg(test)]
#[path = "di_meter_tests.rs"]
mod tests;
