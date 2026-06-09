use super::*;
use domain::value_objects::ParameterValue;

// ── float_or_default ────────────────────────────────────────────

#[test]
fn float_or_default_missing_key_returns_default() {
    let ps = ParameterSet::default();
    let val = float_or_default(&ps, "nonexistent", 42.0).unwrap();
    assert_eq!(val, 42.0);
}

#[test]
fn float_or_default_present_key_returns_value() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(7.5));
    let val = float_or_default(&ps, "gain", 0.0).unwrap();
    assert_eq!(val, 7.5);
}

#[test]
fn float_or_default_wrong_type_returns_error() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::String("not_a_float".into()));
    let result = float_or_default(&ps, "gain", 0.0);
    assert!(result.is_err());
}

#[test]
fn float_or_default_int_value_converts_to_float() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Int(10));
    let val = float_or_default(&ps, "gain", 0.0).unwrap();
    assert_eq!(val, 10.0);
}

// ── bool_or_default ─────────────────────────────────────────────

#[test]
fn bool_or_default_missing_key_returns_default() {
    let ps = ParameterSet::default();
    let val = bool_or_default(&ps, "enabled", true).unwrap();
    assert!(val);
}

#[test]
fn bool_or_default_present_key_returns_value() {
    let mut ps = ParameterSet::default();
    ps.insert("enabled", ParameterValue::Bool(false));
    let val = bool_or_default(&ps, "enabled", true).unwrap();
    assert!(!val);
}

#[test]
fn bool_or_default_wrong_type_returns_error() {
    let mut ps = ParameterSet::default();
    ps.insert("enabled", ParameterValue::Float(1.0));
    let result = bool_or_default(&ps, "enabled", false);
    assert!(result.is_err());
}

#[test]
fn bool_or_default_null_value_returns_error() {
    let mut ps = ParameterSet::default();
    ps.insert("enabled", ParameterValue::Null);
    let result = bool_or_default(&ps, "enabled", false);
    assert!(result.is_err());
}

// ── soft_clip (issue #496) ──────────────────────────────────────

#[test]
fn soft_clip_is_transparent_below_threshold() {
    // A normally-played, well-calibrated model never reaches the
    // threshold: tone and loudness must be byte-identical there.
    for &x in &[0.0, 0.1, -0.25, 0.5, -0.73, 0.8, -0.8] {
        assert_eq!(soft_clip(x), x, "altered a sub-threshold sample {x}");
    }
}

#[test]
fn soft_clip_never_reaches_full_scale_even_for_extreme_input() {
    // Realistic over-range stays strictly inside full-scale; pathological
    // extremes asymptote to exactly ±1.0 (full-scale is valid — clipping
    // is |y| > 1.0 / a flat hard top, which never happens here).
    for &x in &[1.0, 2.73, 8.6, 50.0, -1.0, -3.0] {
        let y = soft_clip(x);
        assert!(y.abs() < 1.0, "soft_clip({x}) = {y} not < 1.0");
        assert!(y.is_finite(), "soft_clip({x}) = {y} not finite");
    }
    for &x in &[1.0e6, -1.0e6, f32::MAX, f32::MIN] {
        let y = soft_clip(x);
        assert!(y.abs() <= 1.0 && y.is_finite(), "soft_clip({x}) = {y} > FS");
    }
}

#[test]
fn soft_clip_is_monotonic_and_odd() {
    let mut prev = f32::NEG_INFINITY;
    let mut x = -10.0;
    while x <= 10.0 {
        let y = soft_clip(x);
        assert!(y > prev, "not strictly increasing at x={x} (y={y})");
        assert!(
            (soft_clip(-x) + y).abs() < 1e-6,
            "not odd-symmetric at x={x}"
        );
        prev = y;
        x += 0.01;
    }
}

#[test]
fn soft_clip_is_continuous_at_the_threshold() {
    // No audible step right where saturation starts.
    let below = soft_clip(0.8 - 1e-4);
    let at = soft_clip(0.8);
    let above = soft_clip(0.8 + 1e-4);
    assert!((at - below).abs() < 1e-3 && (above - at).abs() < 1e-3);
}

// ── soft_clip extended structural battery (issue #496) ─────────

#[test]
fn soft_clip_zero_in_zero_out() {
    assert_eq!(soft_clip(0.0), 0.0);
}
// Note: INF/NaN aren't valid audio samples; finite-input properties
// are covered exhaustively above + by `soft_clip_finite_for_any_finite_input`.
#[test]
fn soft_clip_finite_for_largest_finite_positive() {
    assert!(soft_clip(f32::MAX).is_finite() && soft_clip(f32::MAX).abs() <= 1.0);
}
#[test]
fn soft_clip_finite_for_smallest_finite_negative() {
    assert!(soft_clip(f32::MIN).is_finite() && soft_clip(f32::MIN).abs() <= 1.0);
}
#[test]
fn soft_clip_passes_negative_zero_unchanged() {
    let y = soft_clip(-0.0);
    assert!(y == 0.0);
}
#[test]
fn soft_clip_transparent_at_subnormal() {
    let x = f32::MIN_POSITIVE / 2.0;
    assert_eq!(soft_clip(x), x);
}
#[test]
fn soft_clip_no_jump_dense_positive() {
    let mut prev = soft_clip(0.0);
    let mut x = 1e-4_f32;
    while x <= 3.0 {
        let y = soft_clip(x);
        assert!((y - prev).abs() < 2e-4, "x={x} step");
        prev = y;
        x += 1e-4;
    }
}
#[test]
fn soft_clip_no_jump_dense_negative() {
    let mut prev = soft_clip(0.0);
    let mut x = -1e-4_f32;
    while x >= -3.0 {
        let y = soft_clip(x);
        assert!((y - prev).abs() < 2e-4);
        prev = y;
        x -= 1e-4;
    }
}
#[test]
fn soft_clip_bounded_dense_grid_extreme() {
    for i in 0..=100u32 {
        let x = i as f32 * 100.0;
        assert!(soft_clip(x).abs() <= 1.0);
        assert!(soft_clip(-x).abs() <= 1.0);
    }
}
#[test]
fn soft_clip_strict_below_unity_in_realistic_overdrive() {
    for x in (10..=300u32).map(|i| i as f32 / 100.0) {
        let y = soft_clip(x);
        assert!(y < 1.0, "x={x} y={y}");
    }
}
#[test]
fn soft_clip_above_threshold_strictly_reduces_input() {
    for i in 81..=200u32 {
        let x = i as f32 / 100.0;
        assert!(soft_clip(x) < x, "x={x}");
    }
}
#[test]
fn soft_clip_below_threshold_is_identity_dense() {
    for i in 0..=80u32 {
        let x = i as f32 / 100.0;
        assert_eq!(soft_clip(x), x);
    }
}
#[test]
fn soft_clip_below_threshold_is_identity_dense_negative() {
    for i in 1..=80u32 {
        let x = -(i as f32 / 100.0);
        assert_eq!(soft_clip(x), x);
    }
}
#[test]
fn soft_clip_odd_symmetric_grid() {
    for i in 0..=400u32 {
        let x = i as f32 / 100.0;
        assert!((soft_clip(x) + soft_clip(-x)).abs() < 1e-6);
    }
}
#[test]
fn soft_clip_finite_for_any_finite_input() {
    for x in (-3000i32..=3000i32).map(|i| i as f32 / 100.0) {
        assert!(soft_clip(x).is_finite(), "x={x}");
    }
}
#[test]
fn soft_clip_monotonic_zero_to_three() {
    let mut prev = soft_clip(0.0);
    let mut x = 1e-4_f32;
    while x <= 3.0 {
        let y = soft_clip(x);
        assert!(y + 1e-7 >= prev);
        prev = y;
        x += 1e-4;
    }
}
#[test]
fn soft_clip_monotonic_negative_zero_to_three() {
    let mut prev = soft_clip(0.0);
    let mut x = -1e-4_f32;
    while x >= -3.0 {
        let y = soft_clip(x);
        assert!(y - 1e-7 <= prev);
        prev = y;
        x -= 1e-4;
    }
}
#[test]
fn soft_clip_continuous_at_threshold_dense() {
    for d in (1..=100u32).map(|i| i as f32 / 1_000_000.0) {
        let a = soft_clip(0.8 - d);
        let b = soft_clip(0.8 + d);
        assert!((a - 0.8).abs() < d * 2.0 + 1e-6);
        assert!((b - 0.8).abs() < d * 2.0 + 1e-6);
    }
}
#[test]
fn soft_clip_strict_under_unity_for_huge_input() {
    for x in [1e3_f32, 1e4, 1e5, 1e7] {
        let y = soft_clip(x);
        assert!(y > 0.9 && y <= 1.0);
    }
}
#[test]
fn soft_clip_unity_input_is_above_threshold_response() {
    let y = soft_clip(1.0);
    assert!(y > 0.8 && y < 1.0);
}
#[test]
fn soft_clip_two_input_response_bounded() {
    let y = soft_clip(2.0);
    assert!(y > 0.9 && y < 1.0);
}

// ── peak-safety anti-aliasing (issue #675) ─────────────────────
//
// The existing soft_clip battery above proves the saturator's TIME-domain
// shape (monotonic, odd, bounded, continuous) but never its FREQUENCY
// behavior. The "xiado" bug is in the frequency domain: a base-rate
// nonlinearity folds the harmonics it generates above Nyquist back into
// the audible band as inharmonic aliasing. A loud-calibrated capture (some
// NAMs are baked hot) drives the stage well past 0.8 on normal playing, so
// the fold-back is continuous — heard as harsh hiss. Invariant #2 forbids
// added aliasing, so the peak-safety stage must not inject it.

/// Single-bin magnitude via Goertzel — no FFT dependency, deterministic.
/// With an integer bin (`len * freq / sample_rate` whole) there is no
/// spectral leakage, so a clean tone reads ~0 at any unrelated bin.
fn goertzel_mag(samples: &[f32], freq_hz: f32, sample_rate: f32) -> f32 {
    let n = samples.len() as f32;
    let k = (n * freq_hz / sample_rate).round();
    let w = 2.0 * std::f32::consts::PI * k / n;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in samples {
        let s = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).sqrt()
}

#[test]
fn peak_safety_is_transparent_below_threshold() {
    // Normal playing sits below the 0.8 knee. There soft_clip is the
    // identity, so the saturator must be byte-exact transparent — the
    // anti-aliasing must NOT low-pass clean signal (that would dull the
    // tone: the #413 "abafado" regression, and the #496 transparency
    // contract). Amplitude 0.5 keeps every sample sub-threshold.
    const FS: f32 = 48_000.0;
    let input: Vec<f32> = (0..1024)
        .map(|n| 0.5 * (2.0 * std::f32::consts::PI * 7_500.0 * n as f32 / FS).sin())
        .collect();
    let mut buf = input.clone();
    PeakSafety::new().process_block(&mut buf);
    assert_eq!(buf, input, "peak safety altered sub-threshold (clean) signal");
}

/// Aliasing the saturator folds into the band, as a fraction of the
/// fundamental, for a hot tone at `f0`. `f0` and the measured alias bin
/// both land on exact Goertzel bins at N=4096, so a clean path reads ~0.
fn peak_safety_alias_ratio<F: FnMut(&mut [f32])>(f0: f32, alias_hz: f32, mut saturate: F) -> f32 {
    const FS: f32 = 48_000.0;
    const N: usize = 4096;
    // Hot output: a loud-calibrated model peaks well past the 0.8 knee on
    // normal playing, so the saturator engages continuously.
    let mut buf: Vec<f32> = (0..N)
        .map(|n| 2.0 * (2.0 * std::f32::consts::PI * f0 * n as f32 / FS).sin())
        .collect();
    saturate(&mut buf);
    goertzel_mag(&buf, alias_hz, FS) / goertzel_mag(&buf, f0, FS)
}

#[test]
fn peak_safety_anti_aliases_a_hot_high_tone() {
    // A 7.5 kHz tone (pick-attack / presence-band content, well inside the
    // guitar range) saturated hot. soft_clip is odd, so it makes only odd
    // harmonics; the 5th (37.5 kHz) folds to exactly 10.5 kHz — a bin with
    // ZERO legitimate content, so any energy there is purely the
    // saturator's aliasing. Both 7.5 kHz and 10.5 kHz are exact bins at
    // N=4096 (k=640 / k=896).
    const F0: f32 = 7_500.0;
    const ALIAS: f32 = 10_500.0;

    // The naive per-sample saturator (issue #496) folds audible aliasing
    // into the band — the "xiado". This is the defect.
    let naive = peak_safety_alias_ratio(F0, ALIAS, |buf| {
        for s in buf.iter_mut() {
            *s = soft_clip(*s);
        }
    });
    assert!(
        naive > 0.03,
        "expected the naive saturator to alias audibly, got only {naive:.5}"
    );

    // ADAA band-limits the saturation: same shape, the in-band aliasing
    // collapses by more than an order of magnitude, at zero latency cost.
    let adaa = peak_safety_alias_ratio(F0, ALIAS, |buf| PeakSafety::new().process_block(buf));
    assert!(
        adaa < naive / 5.0 && adaa < 8e-3,
        "ADAA must crush the aliasing: naive={naive:.5} adaa={adaa:.5}"
    );
}
