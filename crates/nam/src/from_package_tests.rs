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

/// New contract: the audit's `manifest.output_gain_db` is NOT added
/// silently to `params.output_level_db` at load time. Whoever creates
/// or loads the block is expected to put the audit value directly
/// into the user-visible param. The UI knob `output_db` then reads
/// what the engine actually applies — no hidden offset.
#[test]
fn manifest_output_gain_db_does_not_stack_onto_user_param() {
    // Mimic the previous behaviour: user set output_db = 0, manifest
    // ships audit = -10 dB. Old code returned -10 (audit auto-stacks).
    // New code returns 0 (audit lives in the preset, not in the
    // loader).
    let resolved = crate::from_package::resolve_user_output_level_db(0.0, Some(-10.0));
    assert_eq!(
        resolved, 0.0,
        "audit must not be summed at load time; the preset carries it"
    );
}

#[test]
fn user_output_param_is_passed_through_when_no_audit_in_manifest() {
    let resolved = crate::from_package::resolve_user_output_level_db(2.5, None);
    assert_eq!(resolved, 2.5);
}

#[test]
fn user_output_param_is_passed_through_even_when_audit_is_present() {
    let resolved = crate::from_package::resolve_user_output_level_db(-3.0, Some(-10.0));
    assert_eq!(resolved, -3.0);
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
