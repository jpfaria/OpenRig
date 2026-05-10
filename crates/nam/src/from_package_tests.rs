//! Tests for `from_package` (issue #402 — NAM gain pedal passthrough at zero).

use super::*;
use domain::value_objects::ParameterValue as BlockParameterValue;
use plugin_loader::manifest::ParameterValue;

fn ts9_like_parameters() -> Vec<GridParameter> {
    let values: Vec<ParameterValue> = (0..=10).map(|n| ParameterValue::Number(n as f64)).collect();
    vec![
        GridParameter {
            name: "drive".to_string(),
            display_name: None,
            values: values.clone(),
        },
        GridParameter {
            name: "tone".to_string(),
            display_name: None,
            values: values.clone(),
        },
        GridParameter {
            name: "level".to_string(),
            display_name: None,
            values,
        },
    ]
}

#[test]
fn knob_at_min_triggers_passthrough() {
    let parameters = ts9_like_parameters();
    let mut ps = ParameterSet::default();
    ps.insert("level", BlockParameterValue::Float(0.0));
    assert!(any_zero_knob(&ps, &parameters));
}

#[test]
fn drive_at_min_triggers_passthrough() {
    let parameters = ts9_like_parameters();
    let mut ps = ParameterSet::default();
    ps.insert("drive", BlockParameterValue::Float(0.0));
    assert!(any_zero_knob(&ps, &parameters));
}

#[test]
fn knob_above_min_does_not_trigger_passthrough() {
    let parameters = ts9_like_parameters();
    let mut ps = ParameterSet::default();
    ps.insert("level", BlockParameterValue::Float(1.0));
    ps.insert("drive", BlockParameterValue::Float(5.0));
    assert!(!any_zero_knob(&ps, &parameters));
}

#[test]
fn user_omits_knob_does_not_trigger_passthrough() {
    let parameters = ts9_like_parameters();
    let ps = ParameterSet::default();
    assert!(!any_zero_knob(&ps, &parameters));
}

#[test]
fn schema_without_zero_knob_does_not_trigger() {
    // Tone-only knob (no drive/level) — passthrough check must skip it.
    let parameters = vec![GridParameter {
        name: "tone".to_string(),
        display_name: None,
        values: vec![ParameterValue::Number(0.0), ParameterValue::Number(10.0)],
    }];
    let mut ps = ParameterSet::default();
    ps.insert("tone", BlockParameterValue::Float(0.0));
    assert!(!any_zero_knob(&ps, &parameters));
}

#[test]
fn manifest_min_picks_lowest_declared_value() {
    let parameters = vec![GridParameter {
        name: "level".to_string(),
        display_name: None,
        values: vec![
            ParameterValue::Number(5.0),
            ParameterValue::Number(0.0),
            ParameterValue::Number(10.0),
        ],
    }];
    assert_eq!(manifest_min(&parameters, "level"), Some(0.0));
}

#[test]
fn passthrough_mono_returns_input_unchanged() {
    let mut p = MonoPassthrough;
    assert_eq!(p.process_sample(0.5), 0.5);
    assert_eq!(p.process_sample(-0.3), -0.3);
}

#[test]
fn passthrough_stereo_returns_input_unchanged() {
    let mut p = StereoPassthrough;
    assert_eq!(p.process_frame([0.5, -0.3]), [0.5, -0.3]);
}
