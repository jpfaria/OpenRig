//! Tests for `from_package` (issue #402 — on-load loudness normalization).

use super::*;

#[test]
fn gain_targets_minus_one_dbfs_when_peak_is_unity() {
    // Capture peaks at 0 dBFS (linear 1.0). To land at -1 dBFS we need
    // to attenuate by 10^(-1/20) ≈ 0.891.
    let g = compute_gain_to_target(1.0, -1.0);
    let expected = 10f32.powf(-1.0 / 20.0);
    assert!((g - expected).abs() < 1e-6, "got {g}, expected {expected}");
}

#[test]
fn gain_amplifies_quiet_capture_to_target() {
    // Capture peaks at -20 dBFS (linear 0.1). Target -1 dBFS means
    // we need to push it up by ~+19 dB ⇒ linear ~8.913.
    let g = compute_gain_to_target(0.1, -1.0);
    let expected = 10f32.powf(-1.0 / 20.0) / 0.1;
    assert!((g - expected).abs() < 1e-3, "got {g}, expected {expected}");
}

#[test]
fn gain_is_unity_for_silent_capture() {
    // A capture stuck at silence produces peak 0; we don't amplify
    // noise — just leave it alone.
    let g = compute_gain_to_target(0.0, -1.0);
    assert_eq!(g, 1.0);
    let g_eps = compute_gain_to_target(1e-12, -1.0);
    assert_eq!(g_eps, 1.0);
}

#[test]
fn gain_is_below_one_for_already_loud_capture() {
    // A hot capture above -1 dBFS must be attenuated, not boosted.
    let g = compute_gain_to_target(2.0, -1.0);
    assert!(g < 1.0, "loud capture should be attenuated, got {g}");
}

#[test]
fn pink_noise_normalized_rms_matches_target_lufs() {
    let n = 48_000;
    let pink = pink_noise_at(-18.0, n);
    let measured = rms(&pink);
    let expected = lufs_to_linear(-18.0);
    assert!(
        (measured - expected).abs() < 0.01,
        "rms {measured:.4} far from target {expected:.4}"
    );
}

#[test]
fn pink_noise_is_deterministic_for_reproducible_probes() {
    // Same seed must yield identical samples — so two probes of the
    // same capture compute the same gain (cache key correctness).
    let a = pink_noise_at(-18.0, 1024);
    let b = pink_noise_at(-18.0, 1024);
    assert_eq!(a, b);
}
