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
mod tests {
    use super::*;

    #[test]
    fn playing_peaks_convert_to_dbfs() {
        let meter = di_meter_from_peaks(Some((0.5, 0.25)), true);
        assert!(
            (meter.in_dbfs - -6.0206).abs() < 0.01,
            "0.5 linear is ~-6.02 dBFS, got {}",
            meter.in_dbfs
        );
        assert!(
            (meter.out_dbfs - -12.0412).abs() < 0.01,
            "0.25 linear is ~-12.04 dBFS, got {}",
            meter.out_dbfs
        );
    }

    #[test]
    fn not_playing_reads_silent() {
        let meter = di_meter_from_peaks(Some((0.5, 0.5)), false);
        assert_eq!(meter.in_dbfs, SILENT_DBFS);
        assert_eq!(meter.out_dbfs, SILENT_DBFS);
    }

    #[test]
    fn render_pending_reads_silent() {
        let meter = di_meter_from_peaks(None, true);
        assert_eq!(meter.in_dbfs, SILENT_DBFS);
        assert_eq!(meter.out_dbfs, SILENT_DBFS);
    }

    #[test]
    fn zero_peak_reads_silent_not_negative_infinity() {
        let meter = di_meter_from_peaks(Some((0.0, 0.0)), true);
        assert_eq!(meter.in_dbfs, SILENT_DBFS);
        assert_eq!(meter.out_dbfs, SILENT_DBFS);
    }
}
