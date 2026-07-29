//! Red-first: a silent analysis window must not read as a healthy tone.
//!
//! Found validating over MCP against the owner's rig: the bundled DI
//! `fabiano-antunes-STRATO-clean` opens with ~3 s of near-silence (peak
//! 0.0002–0.0007). Asking for a 3 s window analysed that silence and the doctor
//! answered `Ok` with every descriptor at 0.0 — a green light meaning "your
//! tone is fine" when the truth is "I heard nothing". The player then trusts a
//! verdict that was never measured on their sound.

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use feature_dsp::tone_descriptors::SymptomLimits;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{diagnose, DIAGNOSE_BLOCK};

const SR: f32 = 48_000.0;

fn init() {
    use std::sync::Once;
    static I: Once = Once::new();
    I.call_once(engine::native_registry::register_all_natives);
}

fn chain() -> Chain {
    let schema = schema_for_block_model("gain", "volume").expect("gain schema");
    let mut ps = ParameterSet::default();
    ps.insert("volume", ParameterValue::Float(80.0));
    Chain {
        id: ChainId("chain:1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("vol".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ps.normalized_against(&schema).expect("normalize"),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

/// One second at `peak` amplitude — a 1 kHz tone, so "quiet" is the only
/// difference from a usable take.
fn tone_at(peak: f32) -> Vec<[f32; 2]> {
    (0..SR as usize)
        .map(|i| {
            let s = peak * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn digital_silence_is_reported_as_no_signal_not_as_a_healthy_tone() {
    init();
    let input = vec![[0.0f32; 2]; SR as usize];

    let result = diagnose(
        &chain(),
        &input,
        SR,
        DIAGNOSE_BLOCK,
        &SymptomLimits::DEFAULT,
    );

    let err = result.expect_err("silence must not produce a verdict");
    assert!(
        err.contains("no usable signal"),
        "the caller must learn nothing was heard, not read a green light: {err}"
    );
}

#[test]
fn the_silent_head_of_a_di_take_is_reported_as_no_signal() {
    init();
    // The level measured in the first seconds of the bundled STRATO DI.
    let input = tone_at(0.0005);

    let err = diagnose(
        &chain(),
        &input,
        SR,
        DIAGNOSE_BLOCK,
        &SymptomLimits::DEFAULT,
    )
    .expect_err("a near-silent window must not produce a verdict");
    assert!(
        err.contains("no usable signal"),
        "analysing the quiet head of a DI must say so: {err}"
    );
}

#[test]
fn a_quiet_but_real_take_is_still_diagnosed() {
    init();
    // Well below a hot take, well above the silence floor: a real performance
    // recorded low must NOT be rejected.
    let report = diagnose(
        &chain(),
        &tone_at(0.05),
        SR,
        DIAGNOSE_BLOCK,
        &SymptomLimits::DEFAULT,
    )
    .expect("a quiet but audible take is still diagnosable");

    assert_eq!(report.symptom, "Ok", "a clean quiet tone is healthy");
}
