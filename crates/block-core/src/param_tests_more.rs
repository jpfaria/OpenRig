//! ParameterSpec strict-mode / builders / coercion tests (issue #792 split
//! from param_tests.rs). Shares test_schema via super::tests.

use super::tests::test_schema;
use super::*;

// ── strict mode (issue #400 bug #5) ──────────────────────────────

#[test]
fn normalized_strict_rejects_unknown_parameters() {
    // Test #5 of issue #400: schema validator in strict mode must reject
    // params that aren't in the schema, surfacing silent schema drift
    // (e.g. preset using legacy `high_cut`/`low_cut` against the new
    // `native_guitar_eq` schema that expects `low/low_mid/high_mid/high`).
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
    ps.insert("legacy_param", ParameterValue::Float(99.0));
    let err = ps.normalized_strict(&schema).unwrap_err();
    assert!(
        err.contains("unknown parameter 'legacy_param'"),
        "strict mode must surface unknown param name; got: {err}"
    );
}

#[test]
fn normalized_strict_accepts_valid_subset() {
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
    let result = ps.normalized_strict(&schema).unwrap();
    assert_eq!(result.get_f32("gain"), Some(75.0));
}

#[test]
fn normalized_strict_still_rejects_missing_required() {
    // Strict mode preserves the existing "missing required" check: if a
    // schema declares a required param without a default, both lenient
    // and strict modes must error.
    let schema = test_schema(vec![float_parameter(
        "gain",
        "Gain",
        None,
        None, // no default → required
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    )]);
    let ps = ParameterSet::default();
    let err = ps.normalized_strict(&schema).unwrap_err();
    assert!(
        err.contains("missing required parameter 'gain'"),
        "strict mode must surface missing required param; got: {err}"
    );
}

// ── Builder functions ───────────────────────────────────────────

#[test]
fn float_parameter_builder() {
    let spec = float_parameter(
        "gain",
        "Gain",
        Some("amp"),
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    assert_eq!(spec.path, "gain");
    assert_eq!(spec.label, "Gain");
    assert_eq!(spec.group, Some("amp".to_string()));
    assert_eq!(spec.widget, ParameterWidget::Knob);
    assert_eq!(spec.unit, ParameterUnit::Percent);
    assert_eq!(spec.default_value, Some(ParameterValue::Float(50.0)));
    assert!(!spec.optional);
    assert!(!spec.allow_empty);
    assert!(matches!(
        spec.domain,
        ParameterDomain::FloatRange {
            min: 0.0,
            max: 100.0,
            step: 1.0
        }
    ));
}

#[test]
fn bool_parameter_builder() {
    let spec = bool_parameter("bright", "Bright", None, Some(true));
    assert_eq!(spec.path, "bright");
    assert_eq!(spec.widget, ParameterWidget::Toggle);
    assert_eq!(spec.unit, ParameterUnit::None);
    assert_eq!(spec.domain, ParameterDomain::Bool);
    assert_eq!(spec.default_value, Some(ParameterValue::Bool(true)));
}

#[test]
fn bool_parameter_builder_no_default() {
    let spec = bool_parameter("mute", "Mute", None, None);
    assert_eq!(spec.default_value, None);
}

#[test]
fn enum_parameter_builder() {
    let spec = enum_parameter(
        "key",
        "Key",
        Some("scale"),
        Some("C"),
        &[("C", "C Major"), ("D", "D Major")],
    );
    assert_eq!(spec.path, "key");
    assert_eq!(spec.group, Some("scale".to_string()));
    assert_eq!(spec.widget, ParameterWidget::Select);
    assert_eq!(
        spec.default_value,
        Some(ParameterValue::String("C".to_string()))
    );
    if let ParameterDomain::Enum { options } = &spec.domain {
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].value, "C");
        assert_eq!(options[0].label, "C Major");
    } else {
        panic!("expected Enum domain");
    }
}

#[test]
fn text_parameter_builder() {
    let spec = text_parameter("name", "Name", None, Some("default"), true);
    assert_eq!(spec.path, "name");
    assert_eq!(spec.widget, ParameterWidget::TextInput);
    assert_eq!(spec.domain, ParameterDomain::Text);
    assert_eq!(
        spec.default_value,
        Some(ParameterValue::String("default".to_string()))
    );
    assert!(spec.optional);
}

#[test]
fn file_path_parameter_builder() {
    let spec = file_path_parameter("ir", "IR File", None, &["wav", "flac"], true);
    assert_eq!(spec.path, "ir");
    assert_eq!(spec.widget, ParameterWidget::FilePicker);
    assert!(spec.optional);
    if let ParameterDomain::FilePath { extensions } = &spec.domain {
        assert_eq!(extensions, &["wav", "flac"]);
    } else {
        panic!("expected FilePath domain");
    }
}

#[test]
fn multi_slider_parameter_builder() {
    let spec = multi_slider_parameter(
        "eq_band",
        "EQ Band",
        Some("eq"),
        Some(0.0),
        -24.0,
        24.0,
        0.1,
        ParameterUnit::Decibels,
    );
    assert_eq!(spec.path, "eq_band");
    assert_eq!(spec.widget, ParameterWidget::MultiSlider);
    assert_eq!(spec.unit, ParameterUnit::Decibels);
    assert!(matches!(
        spec.domain,
        ParameterDomain::FloatRange {
            min: -24.0,
            max: 24.0,
            ..
        }
    ));
}

#[test]
fn curve_editor_parameter_builder() {
    let spec = curve_editor_parameter(
        "freq",
        "Frequency",
        None,
        CurveEditorRole::X,
        Some(1000.0),
        20.0,
        20000.0,
        1.0,
        ParameterUnit::Hertz,
    );
    assert_eq!(spec.path, "freq");
    assert_eq!(
        spec.widget,
        ParameterWidget::CurveEditor {
            role: CurveEditorRole::X
        }
    );
    assert_eq!(spec.unit, ParameterUnit::Hertz);
}

// ── required_f32 / required_bool / required_string ──────────────

#[test]
fn required_f32_success() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(42.0));
    assert_eq!(required_f32(&ps, "gain").unwrap(), 42.0);
}

#[test]
fn required_f32_missing_fails() {
    let ps = ParameterSet::default();
    assert!(required_f32(&ps, "gain").is_err());
}

#[test]
fn required_f32_wrong_type_fails() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::String("hello".to_string()));
    assert!(required_f32(&ps, "gain").is_err());
}

#[test]
fn required_bool_success() {
    let mut ps = ParameterSet::default();
    ps.insert("mute", ParameterValue::Bool(false));
    assert!(!required_bool(&ps, "mute").unwrap());
}

#[test]
fn required_bool_missing_fails() {
    let ps = ParameterSet::default();
    assert!(required_bool(&ps, "mute").is_err());
}

#[test]
fn required_string_success() {
    let mut ps = ParameterSet::default();
    ps.insert("key", ParameterValue::String("C".to_string()));
    assert_eq!(required_string(&ps, "key").unwrap(), "C");
}

#[test]
fn required_string_missing_fails() {
    let ps = ParameterSet::default();
    assert!(required_string(&ps, "key").is_err());
}

#[test]
fn required_string_wrong_type_fails() {
    let mut ps = ParameterSet::default();
    ps.insert("key", ParameterValue::Int(5));
    assert!(required_string(&ps, "key").is_err());
}

// ── optional_string ─────────────────────────────────────────────

#[test]
fn optional_string_present() {
    let mut ps = ParameterSet::default();
    ps.insert("path", ParameterValue::String("foo.wav".to_string()));
    assert_eq!(optional_string(&ps, "path"), Some("foo.wav".to_string()));
}

#[test]
fn optional_string_null() {
    let mut ps = ParameterSet::default();
    ps.insert("path", ParameterValue::Null);
    assert_eq!(optional_string(&ps, "path"), None);
}

#[test]
fn optional_string_missing() {
    let ps = ParameterSet::default();
    assert_eq!(optional_string(&ps, "path"), None);
}

// ── ParameterDomain::value_kind ─────────────────────────────────

#[test]
fn value_kind_returns_correct_labels() {
    assert_eq!(ParameterDomain::Bool.value_kind(), "bool");
    assert_eq!(
        ParameterDomain::IntRange {
            min: 0,
            max: 10,
            step: 1
        }
        .value_kind(),
        "int"
    );
    assert_eq!(
        ParameterDomain::FloatRange {
            min: 0.0,
            max: 1.0,
            step: 0.1
        }
        .value_kind(),
        "float"
    );
    assert_eq!(
        ParameterDomain::Enum { options: vec![] }.value_kind(),
        "enum"
    );
    assert_eq!(ParameterDomain::Text.value_kind(), "string");
    assert_eq!(
        ParameterDomain::FilePath { extensions: vec![] }.value_kind(),
        "path"
    );
}

// ── BlockParameterDescriptor::validate_value ────────────────────

#[test]
fn block_parameter_descriptor_validate_delegates_to_spec() {
    let desc = BlockParameterDescriptor {
        id: ParameterId::for_block_path(&BlockId("test_block".to_string()), "gain"),
        block_id: BlockId("test_block".to_string()),
        effect_type: "preamp".to_string(),
        model: "test".to_string(),
        audio_mode: ModelAudioMode::MonoOnly,
        path: "gain".to_string(),
        label: "Gain".to_string(),
        group: None,
        widget: ParameterWidget::Knob,
        unit: ParameterUnit::Percent,
        domain: ParameterDomain::FloatRange {
            min: 0.0,
            max: 100.0,
            step: 1.0,
        },
        default_value: Some(ParameterValue::Float(50.0)),
        current_value: ParameterValue::Float(50.0),
        optional: false,
        allow_empty: false,
    };
    assert!(desc.validate_value(&ParameterValue::Float(75.0)).is_ok());
    assert!(desc.validate_value(&ParameterValue::Float(200.0)).is_err());
}

// ── ParameterSpec::materialize ──────────────────────────────────

#[test]
fn materialize_creates_correct_descriptor() {
    let spec = float_parameter(
        "gain",
        "Gain",
        Some("amp"),
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    );
    let block_id = BlockId("test_block".to_string());
    let ctx = crate::param::MaterializeContext {
        block_id: &block_id,
        effect_type: "preamp",
        model: "test_model",
        audio_mode: ModelAudioMode::DualMono,
    };
    let desc = spec.materialize(&ctx, ParameterValue::Float(75.0));
    assert_eq!(desc.path, "gain");
    assert_eq!(desc.label, "Gain");
    assert_eq!(desc.group, Some("amp".to_string()));
    assert_eq!(desc.effect_type, "preamp");
    assert_eq!(desc.model, "test_model");
    assert_eq!(desc.audio_mode, ModelAudioMode::DualMono);
    assert_eq!(desc.current_value, ParameterValue::Float(75.0));
    assert_eq!(desc.default_value, Some(ParameterValue::Float(50.0)));
    assert_eq!(desc.block_id, block_id);
}

// ── Serde roundtrip for ParameterSet ────────────────────────────

#[test]
fn parameter_set_serde_roundtrip() {
    let mut ps = ParameterSet::default();
    ps.insert("gain", ParameterValue::Float(50.0));
    ps.insert("bright", ParameterValue::Bool(true));
    let json = serde_json::to_string(&ps).unwrap();
    let back: ParameterSet = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ps);
}

// ── ParameterWidget variants ────────────────────────────────────

#[test]
fn parameter_widget_equality() {
    assert_eq!(ParameterWidget::Knob, ParameterWidget::Knob);
    assert!(ParameterWidget::Knob != ParameterWidget::Toggle);
    assert_eq!(
        ParameterWidget::CurveEditor {
            role: CurveEditorRole::X
        },
        ParameterWidget::CurveEditor {
            role: CurveEditorRole::X
        }
    );
    assert!(
        ParameterWidget::CurveEditor {
            role: CurveEditorRole::X
        } != ParameterWidget::CurveEditor {
            role: CurveEditorRole::Y
        }
    );
}

// ── ParameterUnit variants ──────────────────────────────────────

#[test]
fn parameter_unit_serde_roundtrip() {
    let units = vec![
        ParameterUnit::None,
        ParameterUnit::Decibels,
        ParameterUnit::Hertz,
        ParameterUnit::Milliseconds,
        ParameterUnit::Percent,
        ParameterUnit::Ratio,
        ParameterUnit::Semitones,
    ];
    for unit in units {
        let json = serde_json::to_string(&unit).unwrap();
        let back: ParameterUnit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, unit);
    }
}

// ── ModelParameterSchema serde ──────────────────────────────────

#[test]
fn model_parameter_schema_debug() {
    let schema = test_schema(vec![]);
    let dbg = format!("{:?}", schema);
    assert!(dbg.contains("test_model"));
}

// ── Backward-compat coercion (issue #401) ──────────────────────────

#[test]
fn normalize_coerces_legacy_float_to_enum_string() {
    use crate::param::{enum_parameter, ModelParameterSchema, ParameterSet};
    let schema = ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: "lv2_test".to_string(),
        display_name: "Test".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "mode",
            "Mode",
            None,
            Some("0"),
            &[("0", "Auto"), ("1", "MIDI"), ("2", "Manual")],
        )],
    };
    let mut set = ParameterSet::default();
    // Project saved BEFORE issue #401 stored mode as Float(0.0).
    set.insert("mode", ParameterValue::Float(2.0));
    let normalized = set
        .normalized_against(&schema)
        .expect("legacy float must coerce to enum string");
    assert_eq!(
        normalized.values.get("mode"),
        Some(&ParameterValue::String("2".to_string())),
        "stored mode should be coerced to its matching enum string"
    );
}

#[test]
fn normalize_coerces_legacy_float_to_bool() {
    use crate::param::{bool_parameter, ModelParameterSchema, ParameterSet};
    let schema = ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: "lv2_test".to_string(),
        display_name: "Test".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![bool_parameter("fastmode", "Fast Mode", None, Some(false))],
    };
    let mut set = ParameterSet::default();
    // Pre-#401 behaviour: Bool ports stored as Float.
    set.insert("fastmode", ParameterValue::Float(1.0));
    let normalized = set
        .normalized_against(&schema)
        .expect("legacy float must coerce to bool");
    assert_eq!(
        normalized.values.get("fastmode"),
        Some(&ParameterValue::Bool(true))
    );

    set.insert("fastmode", ParameterValue::Float(0.0));
    let normalized = set
        .normalized_against(&schema)
        .expect("legacy float 0 must coerce to false");
    assert_eq!(
        normalized.values.get("fastmode"),
        Some(&ParameterValue::Bool(false))
    );
}
