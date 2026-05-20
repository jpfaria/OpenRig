use super::*;
use std::sync::Arc;

use engine::output_meter::SILENT_DBFS;
use engine::spsc::SpscRing;

fn ring_with(samples: &[f32]) -> Arc<SpscRing<f32>> {
    let r = Arc::new(SpscRing::<f32>::new(16, 0.0));
    for &s in samples {
        r.push(s);
    }
    r
}

#[test]
fn empty_taps_return_silent_silent() {
    let (i, o) = compute_meter_for_chain(&[], &[]);
    assert_eq!(i, SILENT_DBFS);
    assert_eq!(o, SILENT_DBFS);
}

#[test]
fn input_only_signal_returns_input_db_and_silent_out() {
    let input = vec![ring_with(&[0.5])];
    let (i, o) = compute_meter_for_chain(&input, &[]);
    assert!((i - (-6.02)).abs() < 0.05, "in={i}");
    assert_eq!(o, SILENT_DBFS);
}

#[test]
fn output_only_signal_returns_silent_in_and_output_db() {
    let output: Vec<Arc<SpscRing<f32>>> = vec![ring_with(&[0.25]), ring_with(&[])];
    let (i, o) = compute_meter_for_chain(&[], &output);
    assert_eq!(i, SILENT_DBFS);
    assert!((o - (-12.04)).abs() < 0.05, "out={o}");
}

#[test]
fn both_taps_report_independent_peaks() {
    let input = vec![ring_with(&[0.5])];
    let output = vec![ring_with(&[0.9]), ring_with(&[])];
    let (i, o) = compute_meter_for_chain(&input, &output);
    let want_in = 20.0_f32 * 0.5_f32.log10();
    let want_out = 20.0_f32 * 0.9_f32.log10();
    assert!((i - want_in).abs() < 0.05);
    assert!((o - want_out).abs() < 0.05);
}

#[test]
fn above_full_scale_reports_positive_for_clip_indicator() {
    let output = vec![ring_with(&[1.5]), ring_with(&[])];
    let (_, o) = compute_meter_for_chain(&[], &output);
    assert!(o > 0.0, "above 1.0 should be > 0 dBFS, got {o}");
}
