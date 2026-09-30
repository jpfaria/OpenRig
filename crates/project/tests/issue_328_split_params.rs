//! #328 — the split and mixer knobs (spec §1.2): keys, Ampero defaults and
//! ranges. Every knob lives in `SplitBlock.params` and goes through the
//! ordinary parameter pipeline, so this schema is what the split editor, MIDI
//! mapping, scenes and MCP all read.

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, normalize_split_params, split_param_descriptors, split_param_specs,
    BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B,
    MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, SPLIT_MODE,
};
use project::param::{ParameterDomain, ParameterSet};

#[test]
fn defaults_are_the_ampero_defaults() {
    let params = default_split_params();
    let expected = [
        (SPLIT_MODE, ParameterValue::String("same".into())),
        (LEVEL_TO_A, ParameterValue::Float(100.0)),
        (LEVEL_TO_B, ParameterValue::Float(100.0)),
        (BALANCE_A, ParameterValue::Float(0.0)),
        (BALANCE_B, ParameterValue::Float(0.0)),
        (MIX_LEVEL_A, ParameterValue::Float(100.0)),
        (MIX_LEVEL_B, ParameterValue::Float(100.0)),
        (MIX_PAN_A, ParameterValue::Float(0.0)),
        (MIX_PAN_B, ParameterValue::Float(0.0)),
        (MIX_B_POLARITY, ParameterValue::String("normal".into())),
        (MIX_MASTER, ParameterValue::Float(50.0)),
        (MIX_MASTER_SUM, ParameterValue::Bool(false)),
    ];
    for (key, value) in &expected {
        assert_eq!(params.get(key), Some(value), "default of {key}");
    }
    assert_eq!(
        params.values.len(),
        expected.len(),
        "no knob beyond the spec table"
    );
}

#[test]
fn every_knob_declares_the_range_of_the_spec_table() {
    let specs = split_param_specs();
    let domain = |key: &str| {
        specs
            .iter()
            .find(|s| s.path == key)
            .map(|s| s.domain.clone())
            .unwrap_or_else(|| panic!("no spec for {key}"))
    };
    for key in [LEVEL_TO_A, LEVEL_TO_B, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER] {
        assert!(
            matches!(domain(key), ParameterDomain::FloatRange { min, max, .. } if min == 0.0 && max == 100.0),
            "{key} is 0–100"
        );
    }
    for key in [BALANCE_A, BALANCE_B, MIX_PAN_A, MIX_PAN_B] {
        assert!(
            matches!(domain(key), ParameterDomain::FloatRange { min, max, .. } if min == -50.0 && max == 50.0),
            "{key} is −50…+50"
        );
    }
    let options = |key: &str| match domain(key) {
        ParameterDomain::Enum { options } => {
            options.into_iter().map(|o| o.value).collect::<Vec<_>>()
        }
        other => panic!("{key} is an enum, got {other:?}"),
    };
    assert_eq!(options(SPLIT_MODE), vec!["same", "dual_mono"]);
    assert_eq!(options(MIX_B_POLARITY), vec!["normal", "invert"]);
    assert_eq!(domain(MIX_MASTER_SUM), ParameterDomain::Bool);
}

#[test]
fn normalize_fills_the_missing_knobs_and_keeps_the_set_ones() {
    let mut partial = ParameterSet::default();
    partial.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    let normalized = normalize_split_params(partial).expect("a partial set normalizes");
    assert_eq!(normalized.get_f32(MIX_PAN_A), Some(-50.0));
    assert_eq!(
        normalized.get_f32(MIX_MASTER),
        Some(50.0),
        "a missing knob takes its default"
    );
}

#[test]
fn normalize_rejects_a_knob_out_of_its_range() {
    let mut params = default_split_params();
    params.insert(MIX_MASTER, ParameterValue::Float(150.0));
    let err = normalize_split_params(params).expect_err("150 is outside 0–100");
    assert!(
        err.contains(MIX_MASTER),
        "the error names the knob, got: {err}"
    );
}

#[test]
fn descriptors_address_each_knob_on_the_split_block() {
    let id = BlockId("chain:x:block:split".into());
    let descriptors =
        split_param_descriptors(&id, &default_split_params()).expect("defaults describe");
    let paths: Vec<&str> = descriptors.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            SPLIT_MODE,
            LEVEL_TO_A,
            LEVEL_TO_B,
            BALANCE_A,
            BALANCE_B,
            MIX_LEVEL_A,
            MIX_LEVEL_B,
            MIX_PAN_A,
            MIX_PAN_B,
            MIX_B_POLARITY,
            MIX_MASTER,
            MIX_MASTER_SUM,
        ]
    );
    assert_eq!(descriptors[0].id.0, "chain:x:block:split::split_mode");
    let groups: Vec<Option<&str>> = descriptors.iter().map(|d| d.group.as_deref()).collect();
    assert_eq!(
        &groups[..5],
        &[Some("Split"); 5],
        "the split editor's knobs"
    );
    assert_eq!(
        &groups[5..],
        &[Some("Mixer"); 7],
        "the mixer editor's knobs"
    );
}
