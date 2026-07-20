//! Tests for the symptom → parameter suggestion.
//!
//! The fixture reuses the ablation scenario: a clean volume followed by a fuzz
//! that fizzes a 1 kHz sine. The diagnosis blames the fuzz; the suggestion must
//! then point at a real knob on the fuzz (its `tone` control) and propose a
//! lower value — never invent a parameter the block lacks.

use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::tone_doctor::diagnose;
use crate::tone_doctor_suggestion::suggest;

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

fn block(id: &str, enabled: bool, kind: AudioBlockKind) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind,
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("suggestion-test".into()),
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
    let n = SR as usize;
    (0..n)
        .map(|i| {
            let s = 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn fizz_suggests_lowering_the_fuzz_tone_knob() {
    init_registry();
    let volume = block("vol", true, core("volume", params("volume", &[("volume", 80.0)])));
    let fuzz = block(
        "fuzz",
        true,
        core("fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 70.0), ("level", 50.0)])),
    );
    let c = chain(vec![volume, fuzz]);

    let d = diagnose(&c, SR, &di_sine(), BUF).expect("diagnose runs");
    let s = suggest(&c, &d).expect("a fizzy fuzz yields a suggestion");

    assert_eq!(s.block_index, 1, "the fuzz is the block to adjust");
    assert_eq!(s.param_path, "tone", "tone is the fuzz's high-cut knob");
    assert!(s.suggested < s.current, "the suggestion lowers it: {s:?}");
    // 25 % of the 0..100 range, from the current 70.
    assert!((s.current - 70.0).abs() < 1e-3, "reports the current value");
    assert!((s.suggested - 45.0).abs() < 1e-3, "nudges down by a quarter-range: {s:?}");
}

#[test]
fn healthy_chain_yields_no_suggestion() {
    init_registry();
    let volume = block("vol", true, core("volume", params("volume", &[("volume", 80.0)])));
    let c = chain(vec![volume]);

    let d = diagnose(&c, SR, &di_sine(), BUF).expect("diagnose runs");
    assert!(suggest(&c, &d).is_none(), "nothing to fix on a clean chain");
}

#[test]
fn suggested_value_stays_within_range() {
    init_registry();
    // Fuzz whose tone is already near the floor: the nudge must clamp, not
    // propose a negative value.
    let fuzz = block(
        "fuzz",
        true,
        core("fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 10.0), ("level", 50.0)])),
    );
    let c = chain(vec![fuzz]);

    let d = diagnose(&c, SR, &di_sine(), BUF).expect("diagnose runs");
    if let Some(s) = suggest(&c, &d) {
        assert!(s.suggested >= 0.0, "never below the parameter minimum: {s:?}");
    }
}
