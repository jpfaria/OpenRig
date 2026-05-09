//! Tests for `from_package` (issue #402 — on-load loudness normalization).

use super::*;

#[test]
fn gain_attenuates_loud_capture_to_target() {
    // Capture peaks at 0 dBFS (linear 1.0). Target -6 dBFS ⇒ attenuate
    // to 10^(-6/20) ≈ 0.501.
    let g = compute_gain_to_target(1.0, -6.0);
    let expected = 10f32.powf(-6.0 / 20.0);
    assert!((g - expected).abs() < 1e-6, "got {g}, expected {expected}");
}

#[test]
fn gain_never_amplifies_quiet_capture() {
    // Capture peaks at -20 dBFS (linear 0.1) — already well below the
    // -6 dBFS target. We do NOT amplify (would stack boosts when NAMs
    // chain: TS9 → Bogner caused acoustic feedback at the previous
    // amplifying behavior). Leave it at unity.
    let g = compute_gain_to_target(0.1, -6.0);
    assert!(
        (g - 1.0).abs() < 1e-9,
        "quiet captures must keep natural level, got {g}"
    );
}

#[test]
fn gain_is_unity_for_silent_capture() {
    // A capture stuck at silence produces peak 0 — leave alone.
    let g = compute_gain_to_target(0.0, -6.0);
    assert_eq!(g, 1.0);
    let g_eps = compute_gain_to_target(1e-12, -6.0);
    assert_eq!(g_eps, 1.0);
}

#[test]
fn gain_attenuates_hot_capture_below_target() {
    // A hot capture (peak 2.0 = +6 dBFS) gets attenuated to land at
    // the -6 dBFS ceiling.
    let g = compute_gain_to_target(2.0, -6.0);
    let expected = 10f32.powf(-6.0 / 20.0) / 2.0;
    assert!(
        (g - expected).abs() < 1e-6,
        "hot capture attenuation: got {g}, expected {expected}"
    );
    assert!(g < 1.0);
}

#[test]
fn gain_at_exactly_target_is_unity() {
    // Capture already at -6 dBFS (linear ~0.501) hits the ceiling
    // exactly: target/peak == 1.0, so unity gain.
    let target_linear = 10f32.powf(-6.0 / 20.0);
    let g = compute_gain_to_target(target_linear, -6.0);
    assert!((g - 1.0).abs() < 1e-6, "got {g}");
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
