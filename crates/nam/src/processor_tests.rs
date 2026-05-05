
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
