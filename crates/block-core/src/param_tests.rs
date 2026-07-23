//! Tests for `block-core::param`. Lifted out of `param.rs` so the production
//! file stays under the size cap. Re-attached as `mod tests` of the parent
//! via `#[cfg(test)] #[path = "param_tests.rs"] mod tests;`, so every
//! `super::*` reference resolves unchanged.

use super::*;

// ── Helper: build a simple schema ───────────────────────────────

pub(super) fn test_schema(params: Vec<ParameterSpec>) -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "test".to_string(),
        model: "test_model".to_string(),
        display_name: "Test Model".to_string(),
        audio_mode: ModelAudioMode::MonoOnly,
        parameters: params,
    }
}

// ── ParameterSet basics ─────────────────────────────────────────

#[test]
fn parameter_set_insert_and_get() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(50.0));
    assert_eq!(ps.get("gain"), Some(&ParameterValue::Float(50.0)));
    assert_eq!(ps.get("missing"), None);
}

#[test]
fn parameter_set_get_bool_success() {
    let mut ps = ParameterSet::default();
    ps.insert("bright", ParameterValue::Bool(true));
    assert_eq!(ps.get_bool("bright"), Some(true));
}

#[test]
fn parameter_set_get_bool_wrong_type_returns_none() {
    let mut ps = ParameterSet::default();
    ps.insert("bright", ParameterValue::Float(1.0));
    assert_eq!(ps.get_bool("bright"), None);
}

#[test]
fn parameter_set_get_i64_success() {
    let mut ps = ParameterSet::default();
    ps.insert("count", ParameterValue::Int(42));
    assert_eq!(ps.get_i64("count"), Some(42));
}

#[test]
fn parameter_set_get_f32_from_float() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(2.5));
    assert!((ps.get_f32("gain").unwrap() - 2.5).abs() < 1e-4);
}

#[test]
fn parameter_set_get_f32_from_int() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Int(10));
    assert_eq!(ps.get_f32("gain"), Some(10.0));
}

#[test]
fn parameter_set_get_string_success() {
    let mut ps = ParameterSet::default();
    ps.insert("key", ParameterValue::String("C".to_string()));
    assert_eq!(ps.get_string("key"), Some("C"));
}

#[test]
fn parameter_set_get_string_wrong_type() {
    let mut ps = ParameterSet::default();
    ps.insert("key", ParameterValue::Bool(false));
    assert_eq!(ps.get_string("key"), None);
}

#[test]
fn parameter_set_get_optional_string_present() {
    let mut ps = ParameterSet::default();
    ps.insert("path", ParameterValue::String("foo.wav".to_string()));
    assert_eq!(ps.get_optional_string("path"), Some(Some("foo.wav")));
}

#[test]
fn parameter_set_get_optional_string_null() {
    let mut ps = ParameterSet::default();
    ps.insert("path", ParameterValue::Null);
    assert_eq!(ps.get_optional_string("path"), Some(None));
}

#[test]
fn parameter_set_get_optional_string_missing() {
    let ps = ParameterSet::default();
    assert_eq!(ps.get_optional_string("path"), None);
}

// ── ParameterSpec::validate_value — Float domain ────────────────

#[test]
fn validate_float_in_range_ok() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(50.0)).is_ok());
}

#[test]
fn validate_float_at_min_boundary() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(0.0)).is_ok());
}

#[test]
fn validate_float_at_max_boundary() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(100.0)).is_ok());
}

#[test]
fn validate_float_below_min_fails() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(-1.0)).is_err());
}

#[test]
fn validate_float_above_max_fails() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(101.0)).is_err());
}

#[test]
fn validate_float_step_alignment_ok() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        10.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(30.0)).is_ok());
}

#[test]
fn validate_float_step_misalignment_is_tolerated() {
    // Per `validate_float_range` in
    // `crates/block-core/src/param/schema.rs`, `step` is a UI hint
    // (slider tick resolution), NOT a validation constraint.
    // Continuous sliders, MCP-supplied floats and scene snapshots
    // routinely land between grid points and are still valid
    // signal-wise. Enforcing step alignment broke the runtime on
    // every scene change (user screenshot 21 May 2026). Range
    // bounds remain enforced — `validate_float_range_rejects`
    // covers that.
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        10.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(35.0)).is_ok());
}

#[test]
fn validate_float_zero_step_allows_any_value() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        0.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Float(33.333)).is_ok());
}

#[test]
fn validate_float_accepts_int_value() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        0.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Int(50)).is_ok());
}

#[test]
fn validate_float_rejects_bool() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Bool(true)).is_err());
}

// ── ParameterSpec::validate_value — Bool domain ─────────────────

#[test]
fn validate_bool_true_ok() {
    let spec = bool_parameter("bright", "Bright", None, Some(false));
    assert!(spec.validate_value(&ParameterValue::Bool(true)).is_ok());
}

#[test]
fn validate_bool_false_ok() {
    let spec = bool_parameter("bright", "Bright", None, Some(true));
    assert!(spec.validate_value(&ParameterValue::Bool(false)).is_ok());
}

#[test]
fn validate_bool_rejects_float() {
    let spec = bool_parameter("bright", "Bright", None, None);
    assert!(spec.validate_value(&ParameterValue::Float(1.0)).is_err());
}

// ── ParameterSpec::validate_value — Enum domain ─────────────────

#[test]
fn validate_enum_valid_option_ok() {
    let spec = enum_parameter(
        "key",
        "Key",
        None,
        Some("C"),
        &[("C", "C Major"), ("D", "D Major")],
    );
    assert!(spec
        .validate_value(&ParameterValue::String("C".to_string()))
        .is_ok());
}

#[test]
fn validate_enum_invalid_option_fails() {
    let spec = enum_parameter(
        "key",
        "Key",
        None,
        Some("C"),
        &[("C", "C Major"), ("D", "D Major")],
    );
    assert!(spec
        .validate_value(&ParameterValue::String("E".to_string()))
        .is_err());
}

#[test]
fn validate_enum_rejects_non_string() {
    let spec = enum_parameter("key", "Key", None, Some("C"), &[("C", "C Major")]);
    assert!(spec.validate_value(&ParameterValue::Int(0)).is_err());
}

// ── ParameterSpec::validate_value — Text domain ─────────────────

#[test]
fn validate_text_non_empty_ok() {
    let spec = text_parameter("name", "Name", None, None, false);
    assert!(spec
        .validate_value(&ParameterValue::String("hello".to_string()))
        .is_ok());
}

#[test]
fn validate_text_empty_fails_when_not_allowed() {
    let spec = text_parameter("name", "Name", None, None, false);
    assert!(spec
        .validate_value(&ParameterValue::String("".to_string()))
        .is_err());
}

#[test]
fn validate_text_whitespace_only_fails_when_not_allowed() {
    let spec = text_parameter("name", "Name", None, None, false);
    assert!(spec
        .validate_value(&ParameterValue::String("   ".to_string()))
        .is_err());
}

// ── ParameterSpec::validate_value — FilePath domain ──────────────

#[test]
fn validate_file_path_valid_extension_ok() {
    let spec = file_path_parameter("file", "File", None, &["wav", "mp3"], false);
    assert!(spec
        .validate_value(&ParameterValue::String("/path/to/file.wav".to_string()))
        .is_ok());
}

#[test]
fn validate_file_path_wrong_extension_fails() {
    let spec = file_path_parameter("file", "File", None, &["wav", "mp3"], false);
    assert!(spec
        .validate_value(&ParameterValue::String("/path/to/file.txt".to_string()))
        .is_err());
}

#[test]
fn validate_file_path_case_insensitive_extension() {
    let spec = file_path_parameter("file", "File", None, &["wav"], false);
    assert!(spec
        .validate_value(&ParameterValue::String("file.WAV".to_string()))
        .is_ok());
}

#[test]
fn validate_file_path_no_extensions_allows_any() {
    let spec = file_path_parameter("file", "File", None, &[], false);
    assert!(spec
        .validate_value(&ParameterValue::String("anything.xyz".to_string()))
        .is_ok());
}

#[test]
fn validate_file_path_empty_string_fails() {
    let spec = file_path_parameter("file", "File", None, &["wav"], false);
    assert!(spec
        .validate_value(&ParameterValue::String("".to_string()))
        .is_err());
}

// ── ParameterSpec::validate_value — Null handling ────────────────

#[test]
fn validate_null_on_required_fails() {
    let spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert!(spec.validate_value(&ParameterValue::Null).is_err());
}

#[test]
fn validate_null_on_optional_ok() {
    let mut spec = float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    spec.optional = true;
    assert!(spec.validate_value(&ParameterValue::Null).is_ok());
}

// ── ParameterSpec::validate_value — IntRange domain ──────────────

#[test]
fn validate_int_range_in_range_ok() {
    let spec = ParameterSpec {
        path: "count".to_string(),
        label: "Count".to_string(),
        group: None,
        widget: ParameterWidget::Knob,
        unit: ParameterUnit::None,
        domain: ParameterDomain::IntRange {
            min: 0,
            max: 10,
            step: 2,
        },
        default_value: None,
        optional: false,
        allow_empty: false,
    };
    assert!(spec.validate_value(&ParameterValue::Int(4)).is_ok());
}

#[test]
fn validate_int_range_out_of_range_fails() {
    let spec = ParameterSpec {
        path: "count".to_string(),
        label: "Count".to_string(),
        group: None,
        widget: ParameterWidget::Knob,
        unit: ParameterUnit::None,
        domain: ParameterDomain::IntRange {
            min: 0,
            max: 10,
            step: 1,
        },
        default_value: None,
        optional: false,
        allow_empty: false,
    };
    assert!(spec.validate_value(&ParameterValue::Int(11)).is_err());
}

#[test]
fn validate_int_range_step_misalignment_fails() {
    let spec = ParameterSpec {
        path: "count".to_string(),
        label: "Count".to_string(),
        group: None,
        widget: ParameterWidget::Knob,
        unit: ParameterUnit::None,
        domain: ParameterDomain::IntRange {
            min: 0,
            max: 10,
            step: 3,
        },
        default_value: None,
        optional: false,
        allow_empty: false,
    };
    assert!(spec.validate_value(&ParameterValue::Int(4)).is_err());
    assert!(spec.validate_value(&ParameterValue::Int(6)).is_ok());
}

#[test]
fn validate_int_range_zero_step_allows_any() {
    let spec = ParameterSpec {
        path: "count".to_string(),
        label: "Count".to_string(),
        group: None,
        widget: ParameterWidget::Knob,
        unit: ParameterUnit::None,
        domain: ParameterDomain::IntRange {
            min: 0,
            max: 10,
            step: 0,
        },
        default_value: None,
        optional: false,
        allow_empty: false,
    };
    assert!(spec.validate_value(&ParameterValue::Int(7)).is_ok());
}

// ── normalized_against ──────────────────────────────────────────

#[test]
fn normalized_against_fills_defaults() {
    let schema = test_schema(vec![float_parameter(
        "gain",
        "Gain",
        None,
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    )]);
    let ps = ParameterSet::default();
    let result = ps.normalized_against(&schema).unwrap();
    assert_eq!(result.get_f32("gain"), Some(50.0));
}

#[test]
fn normalized_against_keeps_existing_values() {
    let schema = test_schema(vec![float_parameter(
        "gain",
        "Gain",
        None,
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    )]);
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(75.0));
    let result = ps.normalized_against(&schema).unwrap();
    assert_eq!(result.get_f32("gain"), Some(75.0));
}

#[test]
fn normalized_against_missing_required_without_default_fails() {
    let schema = test_schema(vec![float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    )]);
    let ps = ParameterSet::default();
    assert!(ps.normalized_against(&schema).is_err());
}

#[test]
fn normalized_against_invalid_value_fails() {
    let schema = test_schema(vec![float_parameter(
        "gain",
        "Gain",
        None,
        None,
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    )]);
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(200.0));
    assert!(ps.normalized_against(&schema).is_err());
}

#[test]
fn normalized_against_keeps_unknown_parameters() {
    let schema = test_schema(vec![float_parameter(
        "gain",
        "Gain",
        None,
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    )]);
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(50.0));
    ps.insert("unknown_param", ParameterValue::Float(99.0));
    let result = ps.normalized_against(&schema).unwrap();
    assert_eq!(result.get_f32("unknown_param"), Some(99.0));
}

