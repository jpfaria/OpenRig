//! Tests for the measured (closed-loop) correction.
//!
//! The key guarantee: the suggested value, when applied and re-rendered, must
//! actually bring the offending descriptor back under its healthy limit — the
//! fix is PROVEN, not guessed.

use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use feature_dsp::tone_descriptors::{analyze, FIZZ_RATIO_LIMIT};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::measure_fix;
use crate::offline::render_chain;
use crate::tone_doctor::diagnose;

const SR: f32 = 48_000.0;
const BUF: usize = 64;

fn init() {
    static I: Once = Once::new();
    I.call_once(|| block_gain::register_natives());
}

fn params(model: &str, values: &[(&str, f32)]) -> ParameterSet {
    let schema = schema_for_block_model("gain", model).unwrap();
    let mut ps = ParameterSet::default();
    for (k, v) in values {
        ps.insert(*k, ParameterValue::Float(*v));
    }
    ps.normalized_against(&schema).unwrap()
}

fn core(id: &str, model: &str, ps: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ps,
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("fix-test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
    }
}

fn di_sine() -> Vec<[f32; 2]> {
    (0..SR as usize)
        .map(|i| {
            let s = 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn measured_fix_actually_brings_the_fizz_back_to_health() {
    init();
    let c = chain(vec![
        core("vol", "volume", params("volume", &[("volume", 80.0)])),
        core("fz", "fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 90.0), ("level", 50.0)])),
    ]);
    let input = di_sine();
    let d = diagnose(&c, SR, &input, BUF).expect("diagnose");

    let fix = measure_fix(&c, SR, &input, BUF, &d)
        .expect("measure runs")
        .expect("a fixable fuzz yields a proven correction");

    assert_eq!(fix.block_index, 1, "targets the fuzz");
    assert!(fix.suggested < fix.current, "lowers the knob: {fix:?}");

    // PROOF: apply the suggested value and re-render — the fizz must now be
    // under the healthy limit. This is what "measured, not guessed" means.
    let mut fixed = c.clone();
    project::block::param_writer::set_parameter_number(
        &mut fixed.blocks[fix.block_index],
        &fix.param_path,
        fix.suggested as f64,
    )
    .unwrap();
    let out = render_chain(&fixed, SR, &input, BUF, 4096).unwrap();
    let after = analyze(&out.samples, SR);
    assert!(
        after.fizz_ratio < FIZZ_RATIO_LIMIT,
        "the suggested value must actually clear the fizz: {} (limit {})",
        after.fizz_ratio,
        FIZZ_RATIO_LIMIT
    );
}

#[test]
fn healthy_chain_has_no_fix() {
    init();
    let c = chain(vec![core("vol", "volume", params("volume", &[("volume", 80.0)]))]);
    let input = di_sine();
    let d = diagnose(&c, SR, &input, BUF).expect("diagnose");
    assert!(measure_fix(&c, SR, &input, BUF, &d).expect("runs").is_none());
}
