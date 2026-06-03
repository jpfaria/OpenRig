use super::*;

fn rings(capacity: usize) -> [Arc<SpscRing<f32>>; 2] {
    [
        Arc::new(SpscRing::<f32>::new(capacity, 0.0)),
        Arc::new(SpscRing::<f32>::new(capacity, 0.0)),
    ]
}

#[test]
fn empty_rings_report_silent() {
    let r = rings(16);
    assert_eq!(pop_peak_dbfs(&r), SILENT_DBFS);
}

#[test]
fn single_full_scale_sample_is_zero_dbfs() {
    let r = rings(16);
    r[0].push(1.0);
    let v = pop_peak_dbfs(&r);
    assert!(v.abs() < 0.01, "expected ≈ 0 dBFS, got {v}");
}

#[test]
fn half_full_scale_is_minus_6_dbfs() {
    let r = rings(16);
    r[0].push(0.5);
    let v = pop_peak_dbfs(&r);
    assert!((v - (-6.02)).abs() < 0.05, "got {v}");
}

#[test]
fn quarter_full_scale_is_minus_12_dbfs() {
    let r = rings(16);
    r[1].push(0.25);
    let v = pop_peak_dbfs(&r);
    assert!((v - (-12.04)).abs() < 0.05, "got {v}");
}

#[test]
fn negative_sample_reported_as_magnitude() {
    let r = rings(16);
    r[1].push(-0.7);
    let v = pop_peak_dbfs(&r);
    let expected = 20.0 * 0.7_f32.log10();
    assert!((v - expected).abs() < 0.05, "got {v}");
}

#[test]
fn peak_taken_over_window_of_samples() {
    let r = rings(16);
    r[0].push(0.1);
    r[0].push(0.9);
    r[0].push(0.3);
    let v = pop_peak_dbfs(&r);
    let expected = 20.0 * 0.9_f32.log10();
    assert!((v - expected).abs() < 0.05, "got {v}");
}

#[test]
fn peak_compares_across_l_and_r() {
    let r = rings(16);
    r[0].push(0.2);
    r[1].push(0.6); // R is louder
    let v = pop_peak_dbfs(&r);
    let expected = 20.0 * 0.6_f32.log10();
    assert!((v - expected).abs() < 0.05, "got {v}");
}

#[test]
fn above_full_scale_reports_positive_dbfs_for_clip_indicator() {
    let r = rings(16);
    r[0].push(1.5);
    let v = pop_peak_dbfs(&r);
    assert!(v > 0.0, "above 1.0 should report > 0 dBFS, got {v}");
}

#[test]
fn each_call_drains_only_unseen_samples() {
    let r = rings(16);
    r[0].push(0.9);
    let v1 = pop_peak_dbfs(&r);
    assert!(v1 > -1.5, "first call should see 0.9");
    let v2 = pop_peak_dbfs(&r);
    assert_eq!(v2, SILENT_DBFS, "ring already drained, second call silent");
    r[0].push(0.1);
    let v3 = pop_peak_dbfs(&r);
    let expected = 20.0 * 0.1_f32.log10();
    assert!((v3 - expected).abs() < 0.05, "got {v3}");
}

#[test]
fn very_quiet_sample_reports_correctly() {
    let r = rings(16);
    r[0].push(0.001);
    let v = pop_peak_dbfs(&r);
    let expected = 20.0 * 0.001_f32.log10();
    assert!((v - expected).abs() < 0.05, "got {v}");
}

// ── N-channel variant (issue #496: same helper used for input_tap) ──

#[test]
fn accepts_a_single_channel_slice() {
    let one: Vec<Arc<SpscRing<f32>>> = vec![Arc::new(SpscRing::<f32>::new(8, 0.0))];
    one[0].push(0.5);
    let v = pop_peak_dbfs(&one);
    assert!((v - (-6.02)).abs() < 0.05, "got {v}");
}

#[test]
fn accepts_three_channel_slice() {
    let three: Vec<Arc<SpscRing<f32>>> = (0..3)
        .map(|_| Arc::new(SpscRing::<f32>::new(8, 0.0)))
        .collect();
    three[2].push(0.8);
    let v = pop_peak_dbfs(&three);
    let expected = 20.0 * 0.8_f32.log10();
    assert!((v - expected).abs() < 0.05, "got {v}");
}

#[test]
fn accepts_zero_channel_slice_silent() {
    let none: Vec<Arc<SpscRing<f32>>> = Vec::new();
    assert_eq!(pop_peak_dbfs(&none), SILENT_DBFS);
}

#[test]
fn n_channel_peak_compares_across_all_channels() {
    let four: Vec<Arc<SpscRing<f32>>> = (0..4)
        .map(|_| Arc::new(SpscRing::<f32>::new(8, 0.0)))
        .collect();
    four[0].push(0.2);
    four[1].push(0.3);
    four[2].push(0.9); // loudest
    four[3].push(0.1);
    let v = pop_peak_dbfs(&four);
    let expected = 20.0 * 0.9_f32.log10();
    assert!((v - expected).abs() < 0.05, "got {v}");
}
