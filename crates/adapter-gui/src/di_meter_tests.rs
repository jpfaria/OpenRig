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
