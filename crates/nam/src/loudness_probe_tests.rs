//! Pure (FFI-free) tests for `loudness_probe`. Issue #402.

use super::*;

#[test]
fn pink_noise_is_deterministic_for_same_seed() {
    let a = pink_noise_buffer(1024, 0xC0FFEE);
    let b = pink_noise_buffer(1024, 0xC0FFEE);
    assert_eq!(a, b);
}

#[test]
fn pink_noise_differs_for_different_seeds() {
    let a = pink_noise_buffer(1024, 1);
    let b = pink_noise_buffer(1024, 2);
    assert_ne!(a, b);
}

#[test]
fn pink_noise_normalised_to_target_peak() {
    let buf = pink_noise_buffer(8192, 0xC0FFEE);
    let peak = buf.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    let peak_db = 20.0 * peak.log10();
    assert!(
        (peak_db - PROBE_INPUT_PEAK_DBFS).abs() < 0.01,
        "peak_db={peak_db}, expected ~{PROBE_INPUT_PEAK_DBFS}"
    );
}

#[test]
fn peak_dbfs_unity_returns_zero() {
    let buf = vec![0.5_f32, -1.0, 0.3, -0.7];
    assert!((peak_dbfs(&buf) - 0.0).abs() < 0.01);
}

#[test]
fn peak_dbfs_half_returns_minus_six_db() {
    let buf = vec![0.5_f32, -0.25, 0.1];
    assert!((peak_dbfs(&buf) - (-6.0206)).abs() < 0.01);
}

#[test]
fn peak_dbfs_silent_buffer_returns_floor() {
    let buf = vec![0.0_f32; 1024];
    assert_eq!(peak_dbfs(&buf), -120.0);
}

#[test]
fn compute_offset_typical_case() {
    assert!((compute_offset_db(-10.0) - 7.0).abs() < 0.001);
}

#[test]
fn compute_offset_clamps_to_zero_when_already_loud() {
    // BOOST-ONLY policy: NAM that's already at or above target stays as-is.
    assert_eq!(compute_offset_db(0.0), MIN_OFFSET_DB);
    assert_eq!(compute_offset_db(-3.0), MIN_OFFSET_DB);
    assert_eq!(compute_offset_db(-2.0), MIN_OFFSET_DB);
}

#[test]
fn compute_offset_clamps_to_max_when_extremely_quiet() {
    assert_eq!(compute_offset_db(-50.0), MAX_OFFSET_DB);
    assert_eq!(compute_offset_db(-120.0), MAX_OFFSET_DB);
}

#[test]
fn cache_returns_inserted_value_for_same_path() {
    insert_for_test("test/dummy.nam", 8.5);
    assert_eq!(lookup_cached("test/dummy.nam"), Some(8.5));
}

#[test]
fn cache_returns_none_for_unknown_path() {
    assert_eq!(lookup_cached("test/never_inserted_xyz.nam"), None);
}
