//! Tests for `loudness_norm` (issue #402).

use super::*;

#[test]
fn gain_attenuates_loud_capture_to_target() {
    // Capture peaks at 0 dBFS (linear 1.0). Target -18 dBFS → 0.126.
    let g = compute_gain_to_target(1.0, -18.0);
    let expected = 10f32.powf(-18.0 / 20.0);
    assert!((g - expected).abs() < 1e-6, "got {g}, expected {expected}");
}

#[test]
fn gain_amplifies_quiet_capture_to_target() {
    // Capture at -36 dBFS (linear ~0.0158). Target -18 dBFS → +18 dB.
    let g = compute_gain_to_target(0.015848931, -18.0);
    let expected = 10f32.powf(-18.0 / 20.0) / 0.015848931;
    assert!((g - expected).abs() < 1e-3, "got {g}, expected {expected}");
}

#[test]
fn gain_is_unity_for_silent_capture() {
    // Silence → no amplification of nothing.
    let g = compute_gain_to_target(0.0, -18.0);
    assert_eq!(g, 1.0);
    let g_eps = compute_gain_to_target(1e-12, -18.0);
    assert_eq!(g_eps, 1.0);
}

#[test]
fn gain_at_exactly_target_is_unity() {
    let target_linear = 10f32.powf(-18.0 / 20.0);
    let g = compute_gain_to_target(target_linear, -18.0);
    assert!((g - 1.0).abs() < 1e-6, "got {g}");
}

#[test]
fn pink_noise_normalized_rms_matches_target_lufs() {
    let n = 48_000;
    let pink = pink_noise_at(-12.0, n);
    let measured = rms(&pink);
    let expected = lufs_to_linear(-12.0);
    assert!(
        (measured - expected).abs() < 0.02,
        "rms {measured:.4} far from target {expected:.4}"
    );
}

#[test]
fn pink_noise_is_deterministic_for_reproducible_probes() {
    let a = pink_noise_at(-12.0, 1024);
    let b = pink_noise_at(-12.0, 1024);
    assert_eq!(a, b);
}

#[test]
fn soft_clip_passes_quiet_signals_through() {
    let ceiling = 10f32.powf(-18.0 / 20.0);
    let small = ceiling * 0.1;
    let y = soft_clip_to_ceiling(small, ceiling);
    // Within ~10% of input (rational saturator slightly compresses).
    assert!(
        (y - small).abs() < 0.1 * small,
        "small signal distorted too much: in={small}, out={y}"
    );
}

#[test]
fn soft_clip_caps_anything_at_or_below_ceiling() {
    let ceiling = 10f32.powf(-18.0 / 20.0);
    for input in [-100.0_f32, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0] {
        let y = soft_clip_to_ceiling(input, ceiling);
        assert!(
            y.abs() < ceiling,
            "soft clip failed: input={input}, output={y}, ceiling={ceiling}"
        );
    }
}

#[test]
fn soft_clip_zero_ceiling_yields_silence() {
    let y = soft_clip_to_ceiling(0.5, 0.0);
    assert_eq!(y, 0.0);
}
