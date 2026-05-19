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
