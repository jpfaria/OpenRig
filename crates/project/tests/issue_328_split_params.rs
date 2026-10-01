//! #328 — the split and mixer knobs (spec §1.2, §11): keys, Ampero defaults
//! and ranges, one set per path. Every knob lives in `SplitBlock.params` and
//! goes through the ordinary parameter pipeline, so this schema is what the
//! split editor, MIDI mapping, scenes and MCP all read.

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_param_keys::{balance, level_to, mix_level, mix_pan, mix_polarity};
use project::block::split_params::{
    default_split_params, normalize_split_params, split_param_descriptors, split_param_specs,
    MIX_MASTER, MIX_MASTER_SUM, SPLIT_MODE,
};
use project::param::{ParameterDomain, ParameterSet};

#[test]
fn defaults_are_the_ampero_defaults_for_every_path() {
    for count in [2, 3, 5] {
        let params = default_split_params(count);
        let mut expected = vec![(
            SPLIT_MODE.to_string(),
            ParameterValue::String("same".into()),
        )];
        for i in 0..count {
            expected.push((level_to(i), ParameterValue::Float(100.0)));
            expected.push((balance(i), ParameterValue::Float(0.0)));
            expected.push((mix_level(i), ParameterValue::Float(100.0)));
            expected.push((mix_pan(i), ParameterValue::Float(0.0)));
            expected.push((mix_polarity(i), ParameterValue::String("normal".into())));
        }
        expected.push((MIX_MASTER.to_string(), ParameterValue::Float(50.0)));
        expected.push((MIX_MASTER_SUM.to_string(), ParameterValue::Bool(false)));
        for (key, value) in &expected {
            assert_eq!(
                params.get(key),
                Some(value),
                "default of {key} at {count} paths"
            );
        }
        assert_eq!(
            params.values.len(),
            expected.len(),
            "no knob beyond the spec table at {count} paths"
        );
    }
}

#[test]
fn every_knob_declares_the_range_of_the_spec_table() {
    let specs = split_param_specs(3);
    let domain = |key: &str| {
        specs
            .iter()
            .find(|s| s.path == key)
            .map(|s| s.domain.clone())
            .unwrap_or_else(|| panic!("no spec for {key}"))
    };
    for i in 0..3 {
        for key in [level_to(i), mix_level(i)] {
            assert!(
                matches!(domain(&key), ParameterDomain::FloatRange { min, max, .. } if min == 0.0 && max == 100.0),
                "{key} is 0–100"
            );
        }
        for key in [balance(i), mix_pan(i)] {
            assert!(
                matches!(domain(&key), ParameterDomain::FloatRange { min, max, .. } if min == -50.0 && max == 50.0),
                "{key} is −50…+50"
            );
        }
    }
    assert!(matches!(
        domain(MIX_MASTER),
        ParameterDomain::FloatRange { min, max, .. } if min == 0.0 && max == 100.0
    ));
    let options = |key: &str| match domain(key) {
        ParameterDomain::Enum { options } => {
            options.into_iter().map(|o| o.value).collect::<Vec<_>>()
        }
        other => panic!("{key} is an enum, got {other:?}"),
    };
    assert_eq!(options(SPLIT_MODE), vec!["same", "dual_mono"]);
    assert_eq!(options(&mix_polarity(2)), vec!["normal", "invert"]);
    assert_eq!(domain(MIX_MASTER_SUM), ParameterDomain::Bool);
}

#[test]
fn normalize_fills_the_missing_knobs_and_keeps_the_set_ones() {
    let mut partial = ParameterSet::default();
    partial.insert(&mix_pan(0), ParameterValue::Float(-50.0));
    let normalized = normalize_split_params(partial, 2).expect("a partial set normalizes");
    assert_eq!(normalized.get_f32(&mix_pan(0)), Some(-50.0));
    assert_eq!(
        normalized.get_f32(MIX_MASTER),
        Some(50.0),
        "a missing knob takes its default"
    );
}

#[test]
fn normalize_renames_the_two_path_keys_of_older_files() {
    let mut legacy = ParameterSet::default();
    legacy.insert("mix_pan_b", ParameterValue::Float(25.0));
    legacy.insert("mix_b_polarity", ParameterValue::String("invert".into()));
    let normalized = normalize_split_params(legacy, 2).expect("a legacy set normalizes");
    assert_eq!(normalized.get_f32(&mix_pan(1)), Some(25.0));
    assert_eq!(
        normalized.get(&mix_polarity(1)),
        Some(&ParameterValue::String("invert".into()))
    );
    assert!(normalized.get("mix_pan_b").is_none(), "the old key is gone");
}

#[test]
fn normalize_rejects_a_knob_out_of_its_range() {
    let mut params = default_split_params(2);
    params.insert(MIX_MASTER, ParameterValue::Float(150.0));
    let err = normalize_split_params(params, 2).expect_err("150 is outside 0–100");
    assert!(
        err.contains(MIX_MASTER),
        "the error names the knob, got: {err}"
    );
}

#[test]
fn descriptors_address_each_knob_on_the_split_block() {
    let id = BlockId("chain:x:block:split".into());
    let descriptors =
        split_param_descriptors(&id, &default_split_params(3), 3).expect("defaults describe");
    let paths: Vec<&str> = descriptors.iter().map(|d| d.path.as_str()).collect();
    let mut expected = vec![SPLIT_MODE.to_string()];
    for i in 0..3 {
        expected.push(level_to(i));
        expected.push(balance(i));
    }
    for i in 0..3 {
        expected.push(mix_level(i));
        expected.push(mix_pan(i));
        expected.push(mix_polarity(i));
    }
    expected.push(MIX_MASTER.into());
    expected.push(MIX_MASTER_SUM.into());
    assert_eq!(paths, expected);
    assert_eq!(descriptors[0].id.0, "chain:x:block:split::split_mode");
    let groups: Vec<Option<&str>> = descriptors.iter().map(|d| d.group.as_deref()).collect();
    assert_eq!(
        &groups[..7],
        &[Some("Split"); 7],
        "the split editor's knobs: mode + level and balance per path"
    );
    assert_eq!(
        &groups[7..],
        &[Some("Mixer"); 11],
        "the mixer editor's knobs: level, pan and polarity per path + master"
    );
}
