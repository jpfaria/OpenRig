//! Tests for offline blame-by-ablation.
//!
//! The fixture is a two-block chain: a clean volume block followed by a fuzz.
//! A fuzz turns a 1 kHz sine into a stack of odd harmonics (3/5/7 kHz), which
//! lands squarely in the presence "fizz" band — so the diagnosis MUST name the
//! fuzz block, not the volume, as the source of the fizz, and confirm it by
//! bypass.

use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use feature_dsp::tone_descriptors::{Symptom, SymptomLimits, ToneDescriptors};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{diagnose, is_healthy, metric};

#[test]
fn is_healthy_flips_direction_for_deficit() {
    // Excess: healthy below the limit.
    assert!(is_healthy(0.4, 0.5, false));
    assert!(!is_healthy(0.6, 0.5, false));
    // Deficit: healthy at or above the floor.
    assert!(is_healthy(0.6, 0.5, true));
    assert!(!is_healthy(0.4, 0.5, true));
}

#[test]
fn metric_marks_deficit_symptoms_only_when_their_floor_is_enabled() {
    let d = ToneDescriptors {
        rms_dbfs: -20.0,
        peak_dbfs: -6.0,
        crest_db: 8.0,
        clip_fraction: 0.0,
        fizz_ratio: 0.0,
        mud_ratio: 0.2,
        boom_ratio: 0.0,
    };
    // Disabled by default → no metric to blame/fix.
    assert!(metric(Symptom::Thin, &d, &SymptomLimits::DEFAULT).is_none());
    assert!(metric(Symptom::Squash, &d, &SymptomLimits::DEFAULT).is_none());
    // Enabled floor → deficit metric with the right value + flag.
    let lim = SymptomLimits {
        thin: 0.3,
        squash: 12.0,
        ..SymptomLimits::DEFAULT
    };
    assert_eq!(metric(Symptom::Thin, &d, &lim), Some((0.2, 0.3, true)));
    assert_eq!(metric(Symptom::Squash, &d, &lim), Some((8.0, 12.0, true)));
    // Excess symptoms stay excess (deficit flag false).
    assert_eq!(metric(Symptom::Mud, &d, &lim).map(|(_, _, def)| def), Some(false));
}

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
        id: ChainId("tone-doctor-test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// One second of a 1 kHz sine at half scale, as stereo frames.
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
fn fuzz_after_clean_volume_is_blamed_for_the_fizz() {
    init_registry();
    let volume = block("vol", true, core("volume", params("volume", &[("volume", 80.0)])));
    let fuzz = block(
        "fuzz",
        true,
        core("fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 70.0), ("level", 50.0)])),
    );
    let c = chain(vec![volume, fuzz]); // block index 0 = volume, 1 = fuzz

    let d = diagnose(&c, SR, &di_sine(), BUF).expect("diagnose runs");

    assert_eq!(d.full_symptom, Symptom::Fizz, "the full chain is fizzy: {:?}", d.full_descriptors);
    assert_eq!(d.culprit, Some(1), "the fuzz (block 1), not the volume, is the culprit");
    assert!(d.bypass_resolved, "disabling the fuzz must clear the fizz");
    // The growth curve records both stages, fizz born only once fuzz is added.
    assert_eq!(d.curve.len(), 2, "one stage per enabled processing block");
    assert!(
        d.curve[0].descriptors.fizz_ratio < d.curve[1].descriptors.fizz_ratio,
        "fizz grows when the fuzz is switched on: {:?}",
        d.curve
    );
}

#[test]
fn clean_chain_has_no_culprit() {
    init_registry();
    let volume = block("vol", true, core("volume", params("volume", &[("volume", 80.0)])));
    let c = chain(vec![volume]);

    let d = diagnose(&c, SR, &di_sine(), BUF).expect("diagnose runs");

    assert_eq!(d.full_symptom, Symptom::Ok, "a clean volume chain is healthy: {:?}", d.full_descriptors);
    assert_eq!(d.culprit, None, "nothing to blame");
}

#[test]
fn a_disabled_block_is_never_blamed() {
    init_registry();
    // Fuzz present but bypassed by the user → it is not in the signal, so the
    // chain is clean and the fuzz cannot be the culprit.
    let fuzz = block(
        "fuzz",
        false,
        core("fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 70.0), ("level", 50.0)])),
    );
    let c = chain(vec![fuzz]);

    let d = diagnose(&c, SR, &di_sine(), BUF).expect("diagnose runs");

    assert_eq!(d.full_symptom, Symptom::Ok);
    assert_eq!(d.culprit, None);
    assert!(d.curve.is_empty(), "a bypassed block is not a growth stage");
}
