//! Tests for objective chain-quality measurement. A clean gain chain measures
//! low distortion and a very low noise floor; a fuzz measurably raises THD+N.

use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::measure_quality;

const SR: f32 = 48_000.0;
const BUF: usize = 64;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_gain::register_natives();
    });
}

fn core(model: &str, params: ParameterSet) -> AudioBlockKind {
    AudioBlockKind::Core(CoreBlock {
        effect_type: "gain".into(),
        model: model.into(),
        params,
    })
}

fn params(model: &str, values: &[(&str, f32)]) -> ParameterSet {
    let schema = schema_for_block_model("gain", model).expect("schema must exist");
    let mut ps = ParameterSet::default();
    for (k, v) in values {
        ps.insert(*k, ParameterValue::Float(*v));
    }
    ps.normalized_against(&schema).expect("params must normalize")
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("quality-test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
    }
}

fn block(id: &str, kind: AudioBlockKind) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind,
    }
}

#[test]
fn clean_gain_chain_measures_low_distortion_and_noise() {
    init_registry();
    let c = chain(vec![block("vol", core("volume", params("volume", &[("volume", 80.0)])))]);
    let m = measure_quality(&c, SR, BUF).expect("measure runs");

    assert!(m.thd_n < 0.05, "a clean gain chain has low THD+N: {m:?}");
    assert!(m.noise_floor_dbfs <= -100.0, "and a low noise floor: {m:?}");
    assert_eq!(m.clip_fraction, 0.0, "and does not clip: {m:?}");
    assert!(m.dynamic_range_db > 0.0, "{m:?}");
}

#[test]
fn fuzz_chain_measures_higher_distortion_than_clean() {
    init_registry();
    let clean = chain(vec![block("vol", core("volume", params("volume", &[("volume", 80.0)])))]);
    let fuzzy = chain(vec![block(
        "fuzz",
        core("fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 70.0), ("level", 50.0)])),
    )]);

    let clean_thd = measure_quality(&clean, SR, BUF).expect("clean").thd_n;
    let fuzz_thd = measure_quality(&fuzzy, SR, BUF).expect("fuzz").thd_n;

    assert!(
        fuzz_thd > clean_thd,
        "a fuzz distorts more than a clean gain: fuzz {fuzz_thd} vs clean {clean_thd}"
    );
}
