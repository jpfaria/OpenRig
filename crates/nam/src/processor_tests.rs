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

#[test] fn soft_clip_zero_in_zero_out() { assert_eq!(soft_clip(0.0), 0.0); }
// Note: INF/NaN aren't valid audio samples; finite-input properties
// are covered exhaustively above + by `soft_clip_finite_for_any_finite_input`.
#[test] fn soft_clip_finite_for_largest_finite_positive() { assert!(soft_clip(f32::MAX).is_finite() && soft_clip(f32::MAX).abs() <= 1.0); }
#[test] fn soft_clip_finite_for_smallest_finite_negative() { assert!(soft_clip(f32::MIN).is_finite() && soft_clip(f32::MIN).abs() <= 1.0); }
#[test] fn soft_clip_passes_negative_zero_unchanged() { let y = soft_clip(-0.0); assert!(y == 0.0); }
#[test] fn soft_clip_transparent_at_subnormal() { let x = f32::MIN_POSITIVE / 2.0; assert_eq!(soft_clip(x), x); }
#[test] fn soft_clip_no_jump_dense_positive() { let mut prev = soft_clip(0.0); let mut x = 1e-4_f32; while x <= 3.0 { let y = soft_clip(x); assert!((y-prev).abs() < 2e-4, "x={x} step"); prev=y; x += 1e-4; } }
#[test] fn soft_clip_no_jump_dense_negative() { let mut prev = soft_clip(0.0); let mut x = -1e-4_f32; while x >= -3.0 { let y = soft_clip(x); assert!((y-prev).abs() < 2e-4); prev=y; x -= 1e-4; } }
#[test] fn soft_clip_bounded_dense_grid_extreme() { for i in 0..=100u32 { let x = i as f32 * 100.0; assert!(soft_clip(x).abs() <= 1.0); assert!(soft_clip(-x).abs() <= 1.0); } }
#[test] fn soft_clip_strict_below_unity_in_realistic_overdrive() { for x in (10..=300u32).map(|i| i as f32 / 100.0) { let y = soft_clip(x); assert!(y < 1.0, "x={x} y={y}"); } }
#[test] fn soft_clip_above_threshold_strictly_reduces_input() { for i in 81..=200u32 { let x = i as f32 / 100.0; assert!(soft_clip(x) < x, "x={x}"); } }
#[test] fn soft_clip_below_threshold_is_identity_dense() { for i in 0..=80u32 { let x = i as f32 / 100.0; assert_eq!(soft_clip(x), x); } }
#[test] fn soft_clip_below_threshold_is_identity_dense_negative() { for i in 1..=80u32 { let x = -(i as f32 / 100.0); assert_eq!(soft_clip(x), x); } }
#[test] fn soft_clip_odd_symmetric_grid() { for i in 0..=400u32 { let x = i as f32 / 100.0; assert!((soft_clip(x) + soft_clip(-x)).abs() < 1e-6); } }
#[test] fn soft_clip_finite_for_any_finite_input() { for x in (-3000i32..=3000i32).map(|i| i as f32 / 100.0) { assert!(soft_clip(x).is_finite(), "x={x}"); } }
#[test] fn soft_clip_monotonic_zero_to_three() { let mut prev = soft_clip(0.0); let mut x = 1e-4_f32; while x <= 3.0 { let y = soft_clip(x); assert!(y + 1e-7 >= prev); prev=y; x += 1e-4; } }
#[test] fn soft_clip_monotonic_negative_zero_to_three() { let mut prev = soft_clip(0.0); let mut x = -1e-4_f32; while x >= -3.0 { let y = soft_clip(x); assert!(y - 1e-7 <= prev); prev=y; x -= 1e-4; } }
#[test] fn soft_clip_continuous_at_threshold_dense() { for d in (1..=100u32).map(|i| i as f32 / 1_000_000.0) { let a = soft_clip(0.8 - d); let b = soft_clip(0.8 + d); assert!((a - 0.8).abs() < d*2.0 + 1e-6); assert!((b - 0.8).abs() < d*2.0 + 1e-6); } }
#[test] fn soft_clip_strict_under_unity_for_huge_input() { for x in [1e3_f32, 1e4, 1e5, 1e7] { let y = soft_clip(x); assert!(y > 0.9 && y <= 1.0); } }
#[test] fn soft_clip_unity_input_is_above_threshold_response() { let y = soft_clip(1.0); assert!(y > 0.8 && y < 1.0); }
#[test] fn soft_clip_two_input_response_bounded() { let y = soft_clip(2.0); assert!(y > 0.9 && y < 1.0); }
