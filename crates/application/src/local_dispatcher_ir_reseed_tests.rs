//! Unit tests for the pure IR `output_db` re-seed resolver (#655).

use super::reseed_output_db_for_capture_change;
use domain::value_objects::ParameterValue as DomainValue;
use plugin_loader::manifest::{GridCapture, GridParameter, ParameterValue};
use project::param::ParameterSet;

fn position_axis() -> Vec<GridParameter> {
    vec![GridParameter {
        name: "position".into(),
        display_name: None,
        values: vec![
            ParameterValue::Text("a".into()),
            ParameterValue::Text("b".into()),
        ],
    }]
}

fn captures_a_minus20_b_minus10() -> Vec<GridCapture> {
    vec![
        GridCapture {
            values: [("position".to_string(), ParameterValue::Text("a".into()))]
                .into_iter()
                .collect(),
            file: "a.wav".into(),
            output_gain_db: Some(-20.0),
        },
        GridCapture {
            values: [("position".to_string(), ParameterValue::Text("b".into()))]
                .into_iter()
                .collect(),
            file: "b.wav".into(),
            output_gain_db: Some(-10.0),
        },
    ]
}

fn params_with_position(value: &str) -> ParameterSet {
    let mut params = ParameterSet::default();
    params.insert("position", DomainValue::String(value.to_string()));
    params
}

#[test]
fn axis_change_reseeds_to_newly_selected_capture_audit() {
    // User switched position to "b" (audit -10 dB) → knob re-seeds to -10.
    let result = reseed_output_db_for_capture_change(
        &position_axis(),
        &captures_a_minus20_b_minus10(),
        None,
        &params_with_position("b"),
        "position",
    );
    assert_eq!(result, Some(-10.0));
}

#[test]
fn dragging_output_db_itself_never_reseeds() {
    // The user is dragging the Output knob — must NOT clobber their value.
    let result = reseed_output_db_for_capture_change(
        &position_axis(),
        &captures_a_minus20_b_minus10(),
        None,
        &params_with_position("b"),
        "output_db",
    );
    assert_eq!(result, None);
}

#[test]
fn capture_without_audit_falls_back_to_manifest_level() {
    let captures = vec![GridCapture {
        values: [("position".to_string(), ParameterValue::Text("a".into()))]
            .into_iter()
            .collect(),
        file: "a.wav".into(),
        output_gain_db: None,
    }];
    let result = reseed_output_db_for_capture_change(
        &position_axis(),
        &captures,
        Some(-4.5),
        &params_with_position("a"),
        "position",
    );
    assert_eq!(result, Some(-4.5));
}
