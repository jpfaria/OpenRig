
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
use domain::value_objects::ParameterValue;

use crate::{
    build_wah_processor_for_layout, supported_models, validate_wah_params, wah_brand,
    wah_display_name, wah_model_schema, wah_model_visual, wah_type_label,
};

// ── helpers ──────────────────────────────────────────────────────

fn default_params(model: &str) -> ParameterSet {
    let schema = wah_model_schema(model).expect("schema should exist");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults should normalize")
}

// ── supported_models ─────────────────────────────────────────────

#[test]
fn supported_models_is_not_empty() {
    assert!(!supported_models().is_empty());
}

#[test]
fn supported_wah_models_expose_valid_schema() {
    for model in supported_models() {
        let schema = wah_model_schema(model)
            .unwrap_or_else(|e| panic!("schema for '{}' failed: {}", model, e));
        assert_eq!(schema.effect_type, "wah", "wrong effect_type for {}", model);
        assert_eq!(schema.model, *model, "schema.model mismatch for {}", model);
    }
}

#[test]
fn supported_models_all_have_visual_data() {
    for model in supported_models() {
        let visual = wah_model_visual(model);
        assert!(visual.is_some(), "missing visual data for {}", model);
    }
}

// ── cry_classic schema ───────────────────────────────────────────

#[test]
fn cry_classic_schema_has_expected_params() {
    let schema = wah_model_schema("cry_classic").unwrap();
    let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
    assert!(paths.contains(&"position"));
    assert!(paths.contains(&"q"));
    assert!(paths.contains(&"mix"));
    assert!(paths.contains(&"output"));
    assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
}

#[test]
fn cry_classic_schema_effect_type_is_wah() {
    let schema = wah_model_schema("cry_classic").unwrap();
    assert_eq!(schema.effect_type, "wah");
    assert_eq!(schema.model, "cry_classic");
}

// ── cry_classic validate ─────────────────────────────────────────

#[test]
fn cry_classic_validate_accepts_defaults() {
    let params = default_params("cry_classic");
    validate_wah_params("cry_classic", &params).expect("defaults should validate");
}

#[test]
fn cry_classic_validate_rejects_out_of_range() {
    let schema = wah_model_schema("cry_classic").unwrap();
    let mut ps = ParameterSet::default();
    ps.insert("position", ParameterValue::Float(200.0)); // out of range
    ps.insert("q", ParameterValue::Float(15.0));
    ps.insert("mix", ParameterValue::Float(100.0));
    ps.insert("output", ParameterValue::Float(50.0));
    assert!(ps.normalized_against(&schema).is_err());
}

#[test]
fn cry_classic_validate_rejects_negative_q() {
    let schema = wah_model_schema("cry_classic").unwrap();
    let mut ps = ParameterSet::default();
    ps.insert("position", ParameterValue::Float(55.0));
    ps.insert("q", ParameterValue::Float(-5.0)); // out of range
    ps.insert("mix", ParameterValue::Float(100.0));
    ps.insert("output", ParameterValue::Float(50.0));
    assert!(ps.normalized_against(&schema).is_err());
}

// ── cry_classic build ────────────────────────────────────────────

#[test]
fn cry_classic_build_mono() {
    let params = default_params("cry_classic");
    let proc =
        build_wah_processor_for_layout("cry_classic", &params, 48000.0, AudioChannelLayout::Mono);
    assert!(proc.is_ok());
    assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
}

#[test]
fn cry_classic_build_stereo() {
    let params = default_params("cry_classic");
    let proc =
        build_wah_processor_for_layout("cry_classic", &params, 48000.0, AudioChannelLayout::Stereo);
    assert!(proc.is_ok());
    assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
}

// ── cry_classic process ──────────────────────────────────────────

#[test]
fn cry_classic_process_mono_produces_non_nan() {
    let params = default_params("cry_classic");
    let mut proc =
        build_wah_processor_for_layout("cry_classic", &params, 48000.0, AudioChannelLayout::Mono)
            .unwrap();
    match &mut proc {
        BlockProcessor::Mono(p) => {
            for i in 0..256 {
                let input = if i == 0 { 1.0 } else { 0.0 };
                let out = p.process_sample(input);
                assert!(!out.is_nan(), "NaN at frame {} for cry_classic mono", i);
            }
        }
        _ => panic!("expected Mono processor"),
    }
}

#[test]
fn cry_classic_process_stereo_produces_non_nan() {
    let params = default_params("cry_classic");
    let mut proc =
        build_wah_processor_for_layout("cry_classic", &params, 48000.0, AudioChannelLayout::Stereo)
            .unwrap();
    match &mut proc {
        BlockProcessor::Stereo(p) => {
            for i in 0..256 {
                let input = if i == 0 { 1.0 } else { 0.0 };
                let [l, r] = p.process_frame([input, input]);
                assert!(!l.is_nan(), "NaN L at frame {}", i);
                assert!(!r.is_nan(), "NaN R at frame {}", i);
            }
        }
        _ => panic!("expected Stereo processor"),
    }
}

#[test]
fn cry_classic_process_silence_yields_silence() {
    let params = default_params("cry_classic");
    let mut proc =
        build_wah_processor_for_layout("cry_classic", &params, 48000.0, AudioChannelLayout::Mono)
            .unwrap();
    match &mut proc {
        BlockProcessor::Mono(p) => {
            for _ in 0..256 {
                let out = p.process_sample(0.0);
                assert_eq!(out, 0.0, "silence in should produce silence out");
            }
        }
        _ => panic!("expected Mono processor"),
    }
}

// ── display_name / brand / type_label ────────────────────────────

#[test]
fn cry_classic_display_name_matches() {
    assert_eq!(wah_display_name("cry_classic"), "Cry Classic");
}

#[test]
fn cry_classic_type_label_is_native() {
    assert_eq!(wah_type_label("cry_classic"), "NATIVE");
}

#[test]
fn unknown_model_returns_empty_strings() {
    assert_eq!(wah_display_name("nonexistent"), "");
    assert_eq!(wah_brand("nonexistent"), "");
    assert_eq!(wah_type_label("nonexistent"), "");
}
