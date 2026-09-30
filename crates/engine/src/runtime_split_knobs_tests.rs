//! #328 spec §1.2: the split's ParameterSet read as the values the audio
//! thread uses — percent knobs become linear gains, enum knobs become flags.

use block_core::param::ParameterSet;
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    self, default_split_params, POLARITY_INVERT, SPLIT_MODE_DUAL_MONO,
};

use super::SplitKnobs;
use crate::runtime_split::mix::SplitKnobValues;

fn ampero_defaults() -> SplitKnobValues {
    SplitKnobValues {
        dual_mono: false,
        level_to_a: 1.0,
        level_to_b: 1.0,
        balance_a: 0.0,
        balance_b: 0.0,
        mix_level_a: 1.0,
        mix_level_b: 1.0,
        mix_pan_a: 0.0,
        mix_pan_b: 0.0,
        mix_b_invert: false,
        mix_master: 0.5,
        mix_master_sum: false,
    }
}

fn with(overrides: &[(&str, ParameterValue)]) -> ParameterSet {
    let mut params = default_split_params();
    for (key, value) in overrides {
        params.insert(*key, value.clone());
    }
    params
}

#[test]
fn defaults_read_as_the_ampero_defaults() {
    let values = SplitKnobs::from_params(&default_split_params()).load(true);
    assert_eq!(values, ampero_defaults());
}

#[test]
fn a_missing_knob_falls_back_to_its_default() {
    let values = SplitKnobs::from_params(&ParameterSet::default()).load(true);
    assert_eq!(values, ampero_defaults());
}

#[test]
fn percent_knobs_become_linear_gains() {
    let params = with(&[
        (split_params::LEVEL_TO_B, ParameterValue::Float(50.0)),
        (split_params::MIX_LEVEL_A, ParameterValue::Float(25.0)),
        (split_params::MIX_MASTER, ParameterValue::Float(100.0)),
    ]);
    let values = SplitKnobs::from_params(&params).load(true);
    assert_eq!(
        (values.level_to_b, values.mix_level_a, values.mix_master),
        (0.5, 0.25, 1.0)
    );
}

#[test]
fn mode_polarity_balance_pan_and_sum_follow_their_values() {
    let params = with(&[
        (
            split_params::SPLIT_MODE,
            ParameterValue::String(SPLIT_MODE_DUAL_MONO.into()),
        ),
        (
            split_params::MIX_B_POLARITY,
            ParameterValue::String(POLARITY_INVERT.into()),
        ),
        (split_params::MIX_MASTER_SUM, ParameterValue::Bool(true)),
        (split_params::BALANCE_A, ParameterValue::Float(-50.0)),
        (split_params::MIX_PAN_B, ParameterValue::Float(50.0)),
    ]);
    let values = SplitKnobs::from_params(&params).load(true);
    assert!(values.dual_mono, "split_mode dual_mono is Mode II");
    assert!(values.mix_b_invert, "mix_b_polarity invert flips path B");
    assert!(values.mix_master_sum);
    assert_eq!((values.balance_a, values.mix_pan_b), (-50.0, 50.0));
}

#[test]
fn a_y_split_reads_a_neutral_mixer_but_keeps_its_split_knobs() {
    let params = with(&[
        (split_params::LEVEL_TO_B, ParameterValue::Float(50.0)),
        (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
        (
            split_params::MIX_B_POLARITY,
            ParameterValue::String(POLARITY_INVERT.into()),
        ),
        (split_params::MIX_MASTER, ParameterValue::Float(10.0)),
    ]);
    let values = SplitKnobs::from_params(&params).load(false);
    assert_eq!(values.level_to_b, 0.5);
    assert_eq!(
        (values.mix_pan_a, values.mix_b_invert, values.mix_master),
        (0.0, false, 1.0)
    );
}
