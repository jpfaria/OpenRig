//! #328 spec §1.2, §11.2: the split's ParameterSet read as the values the
//! audio thread uses — percent knobs become linear gains, enum knobs become
//! flags, one set per path.

use block_core::param::ParameterSet;
use domain::value_objects::ParameterValue;
use project::block::split_param_keys::{balance, level_to, mix_level, mix_pan, mix_polarity};
use project::block::split_params::{
    default_split_params, MIX_MASTER, MIX_MASTER_SUM, POLARITY_INVERT, SPLIT_MODE,
    SPLIT_MODE_DUAL_MONO,
};

use super::SplitKnobs;
use crate::runtime_split::mix::{MixKnobs, PathKnobs};

fn ampero_mix() -> MixKnobs {
    MixKnobs {
        dual_mono: false,
        master: 0.5,
        master_sum: false,
    }
}

fn with(count: usize, overrides: &[(String, ParameterValue)]) -> ParameterSet {
    let mut params = default_split_params(count);
    for (key, value) in overrides {
        params.insert(key, value.clone());
    }
    params
}

/// The values one callback reads from `params` on a split of `count` paths.
fn load(params: &ParameterSet, count: usize, mixes: bool) -> (Vec<PathKnobs>, MixKnobs) {
    let knobs = SplitKnobs::from_params(params, count);
    let mut paths = vec![PathKnobs::neutral(); count];
    let mix = knobs.load_into(mixes, &mut paths);
    (paths, mix)
}

#[test]
fn defaults_read_as_the_ampero_defaults() {
    for count in [2, 3, 5] {
        let (paths, mix) = load(&default_split_params(count), count, true);
        assert_eq!(paths, vec![PathKnobs::neutral(); count], "{count} paths");
        assert_eq!(mix, ampero_mix());
    }
}

#[test]
fn a_missing_knob_falls_back_to_its_default() {
    let (paths, mix) = load(&ParameterSet::default(), 3, true);
    assert_eq!(paths, vec![PathKnobs::neutral(); 3]);
    assert_eq!(mix, ampero_mix());
}

#[test]
fn percent_knobs_become_linear_gains() {
    let params = with(
        3,
        &[
            (level_to(1), ParameterValue::Float(50.0)),
            (mix_level(0), ParameterValue::Float(25.0)),
            (mix_level(2), ParameterValue::Float(75.0)),
            (MIX_MASTER.into(), ParameterValue::Float(100.0)),
        ],
    );
    let (paths, mix) = load(&params, 3, true);
    assert_eq!(
        (
            paths[1].level_to,
            paths[0].mix_level,
            paths[2].mix_level,
            mix.master
        ),
        (0.5, 0.25, 0.75, 1.0)
    );
}

#[test]
fn mode_polarity_balance_pan_and_sum_follow_their_values() {
    let params = with(
        3,
        &[
            (
                SPLIT_MODE.into(),
                ParameterValue::String(SPLIT_MODE_DUAL_MONO.into()),
            ),
            (
                mix_polarity(2),
                ParameterValue::String(POLARITY_INVERT.into()),
            ),
            (MIX_MASTER_SUM.into(), ParameterValue::Bool(true)),
            (balance(0), ParameterValue::Float(-50.0)),
            (mix_pan(1), ParameterValue::Float(50.0)),
        ],
    );
    let (paths, mix) = load(&params, 3, true);
    assert!(mix.dual_mono, "split_mode dual_mono is Mode II");
    assert_eq!(
        paths.iter().map(|p| p.invert).collect::<Vec<_>>(),
        vec![false, false, true],
        "each path has its own polarity"
    );
    assert!(mix.master_sum);
    assert_eq!((paths[0].balance, paths[1].mix_pan), (-50.0, 50.0));
}

#[test]
fn a_y_split_reads_a_neutral_mixer_but_keeps_its_split_knobs() {
    let params = with(
        2,
        &[
            (level_to(1), ParameterValue::Float(50.0)),
            (mix_pan(0), ParameterValue::Float(-50.0)),
            (
                mix_polarity(1),
                ParameterValue::String(POLARITY_INVERT.into()),
            ),
            (MIX_MASTER.into(), ParameterValue::Float(10.0)),
        ],
    );
    let (paths, mix) = load(&params, 2, false);
    assert_eq!(paths[1].level_to, 0.5);
    assert_eq!(
        (paths[0].mix_pan, paths[1].invert, mix.master),
        (0.0, false, 1.0)
    );
}

#[test]
fn a_store_replaces_every_path_in_place() {
    let knobs = SplitKnobs::from_params(&default_split_params(3), 3);
    knobs.store(&with(3, &[(mix_level(2), ParameterValue::Float(0.0))]));
    let mut paths = vec![PathKnobs::neutral(); 3];
    knobs.load_into(true, &mut paths);
    assert_eq!(
        paths.iter().map(|p| p.mix_level).collect::<Vec<_>>(),
        vec![1.0, 1.0, 0.0]
    );
}
