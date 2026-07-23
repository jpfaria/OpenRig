//! Tests for the Tone Doctor GUI wiring (pure functions).

use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::*;

const SR: f32 = 48_000.0;
const BUF: usize = 64;

fn init() {
    static I: Once = Once::new();
    I.call_once(|| engine::native_registry::register_all_natives());
}

fn params(model: &str, values: &[(&str, f32)]) -> ParameterSet {
    let schema = schema_for_block_model("gain", model).unwrap();
    let mut ps = ParameterSet::default();
    for (k, v) in values {
        ps.insert(*k, ParameterValue::Float(*v));
    }
    ps.normalized_against(&schema).unwrap()
}

fn core_block(id: &str, model: &str, ps: ParameterSet) -> AudioBlock {
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
        id: ChainId("c".into()),
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

fn di_sine() -> Vec<[f32; 2]> {
    (0..SR as usize)
        .map(|i| {
            let s = 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn fuzz_chain_view_reports_fizz_culprit_and_suggestion() {
    init();
    let c = chain(vec![
        core_block("vol", "volume", params("volume", &[("volume", 80.0)])),
        core_block("fz", "fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 70.0), ("level", 50.0)])),
    ]);
    let (view, suggestion) = diagnose_to_view(&c, &di_sine(), SR, BUF, feature_dsp::tone_descriptors::SymptomLimits::DEFAULT);

    assert!(view.has_result, "{view:?}");
    assert!(!view.running, "run completed: {view:?}");
    assert_eq!(view.symptom_level, 2, "fizz is a red-level symptom: {view:?}");
    assert_eq!(view.symptom_text, "Fizz", "{view:?}");
    assert_eq!(
        view.culprit_label,
        project::catalog::model_display_name("gain", "fuzz_si"),
        "the panel shows the plugin's display name, not the id: {view:?}"
    );
    assert!(view.has_suggestion, "{view:?}");
    assert!(!view.suggestion_text.is_empty(), "shows the measured move: {view:?}");
    // The measurements travel to the panel meters.
    assert!(view.fizz_value > view.fizz_limit, "fizz over its limit: {view:?}");
    assert_eq!(view.fizz_limit, feature_dsp::tone_descriptors::FIZZ_RATIO_LIMIT);
    // Measured, not guessed: the fix targets the culprit and lowers a knob.
    // (Which knob is whichever measurably clears the fizz — proven in engine's
    // tone_doctor_fix tests.)
    let s = suggestion.expect("a suggestion is cached for Apply");
    assert_eq!(s.block_index, 1, "targets the fuzz");
    assert!(s.suggested < s.current, "lowers it: {s:?}");
}

#[test]
fn healthy_chain_view_has_no_result_flag_set_but_no_culprit() {
    init();
    let c = chain(vec![core_block("vol", "volume", params("volume", &[("volume", 80.0)]))]);
    let (view, suggestion) = diagnose_to_view(&c, &di_sine(), SR, BUF, feature_dsp::tone_descriptors::SymptomLimits::DEFAULT);

    assert!(view.has_result, "a run happened: {view:?}");
    assert_eq!(view.symptom_level, 0, "healthy = green: {view:?}");
    assert_eq!(view.symptom_text, "OK", "{view:?}");
    assert!(view.culprit_label.is_empty(), "{view:?}");
    assert!(!view.has_suggestion, "{view:?}");
    assert!(suggestion.is_none(), "{view:?}");
}

#[test]
fn apply_command_targets_the_culprit_block() {
    init();
    let c = chain(vec![
        core_block("vol", "volume", params("volume", &[("volume", 80.0)])),
        core_block("fz", "fuzz_si", params("fuzz_si", &[("fuzz", 95.0), ("tone", 70.0), ("level", 50.0)])),
    ]);
    let (_view, suggestion) = diagnose_to_view(&c, &di_sine(), SR, BUF, feature_dsp::tone_descriptors::SymptomLimits::DEFAULT);
    let s = suggestion.expect("suggestion");
    let cmds = apply_commands(&c, &c.id, &s);
    // A native fuzz knob is always live (no enable gate) → a single command.
    assert_eq!(cmds.len(), 1, "{cmds:?}");
    match &cmds[0] {
        Command::SetBlockParameterNumber { chain, block, path, value } => {
            assert_eq!(*chain, c.id);
            assert_eq!(*block, BlockId("fz".into()), "targets the fuzz");
            assert_eq!(path, &s.param_path);
            assert!((*value as f32) < s.current, "lowers the knob: {value}");
        }
        other => panic!("wrong command: {other:?}"),
    }
}

#[test]
fn culprit_label_shows_the_plugin_name_not_the_internal_id() {
    init();
    let c = chain(vec![core_block("fz", "fuzz_si", params("fuzz_si", &[]))]);
    let label = culprit_label(&c, Some(0));
    let friendly = project::catalog::model_display_name("gain", "fuzz_si");
    assert!(!friendly.is_empty(), "the model has a display name");
    assert_eq!(label, friendly, "the panel shows the plugin's display name");
    assert_ne!(label, "gain:fuzz_si", "never the internal effect:model id");
}
