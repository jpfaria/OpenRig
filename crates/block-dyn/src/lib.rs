//! Dynamics implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DynBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn dyn_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            DynBackendKind::Native => "NATIVE",
            DynBackendKind::Nam => "NAM",
            DynBackendKind::Ir => "IR",
            DynBackendKind::Lv2 => "LV2",
            DynBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn dyn_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn dyn_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn dyn_type_label(model: &str) -> &'static str {
    dyn_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn compressor_supported_models() -> &'static [&'static str] {
    registry::COMPRESSOR_SUPPORTED_MODELS
}

pub fn gate_supported_models() -> &'static [&'static str] {
    registry::GATE_SUPPORTED_MODELS
}

pub fn dynamics_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_dynamics_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_dynamics_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_dynamics_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

pub fn compressor_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_compressor_model_definition(model)?.schema)()
}

pub fn build_compressor_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_compressor_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_compressor_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_compressor_model_definition(model)?.build)(params, sample_rate, layout)
}

pub fn gate_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_gate_model_definition(model)?.schema)()
}

pub fn build_gate_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_gate_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gate_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_gate_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
    use domain::value_objects::ParameterValue;

    // ── Helper ──────────────────────────────────────────────────────

    fn default_params_for(model: &str) -> ParameterSet {
        let schema = dynamics_model_schema(model).expect("schema should exist");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    fn process_silence(processor: &mut BlockProcessor, frames: usize) -> Vec<f32> {
        match processor {
            BlockProcessor::Mono(p) => {
                let mut buf = vec![0.0_f32; frames];
                p.process_block(&mut buf);
                buf
            }
            BlockProcessor::Stereo(p) => {
                let mut buf = vec![[0.0_f32; 2]; frames];
                p.process_block(&mut buf);
                buf.iter().flat_map(|pair| pair.iter().copied()).collect()
            }
        }
    }

    // ── Registry-level tests ────────────────────────────────────────

    #[test]
    fn supported_dyn_models_expose_schema() {
        for model in supported_models() {
            assert!(
                dynamics_model_schema(model).is_ok(),
                "expected '{model}' to have a valid schema"
            );
        }
    }

    #[test]
    fn compressor_supported_models_is_subset_of_all() {
        let all = supported_models();
        for model in compressor_supported_models() {
            assert!(
                all.contains(model),
                "compressor model '{model}' missing from supported_models"
            );
        }
    }

    #[test]
    fn gate_supported_models_is_subset_of_all() {
        let all = supported_models();
        for model in gate_supported_models() {
            assert!(
                all.contains(model),
                "gate model '{model}' missing from supported_models"
            );
        }
    }

    // ── Compressor: Studio Clean ────────────────────────────────────

    #[test]
    fn compressor_studio_clean_schema_has_expected_params() {
        let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert_eq!(schema.model, "compressor_studio_clean");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"threshold"));
        assert!(param_names.contains(&"ratio"));
        assert!(param_names.contains(&"attack_ms"));
        assert!(param_names.contains(&"release_ms"));
        assert!(param_names.contains(&"makeup_gain"));
        assert!(param_names.contains(&"mix"));
    }

    #[test]
    fn compressor_studio_clean_defaults_normalize() {
        let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok(), "defaults should normalize");
    }

    #[test]
    fn compressor_studio_clean_rejects_out_of_range() {
        let schema = dynamics_model_schema("compressor_studio_clean").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("threshold", ParameterValue::Float(200.0)); // max is 100
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn compressor_studio_clean_build_mono() {
        let params = default_params_for("compressor_studio_clean");
        let proc = build_dynamics_processor_for_layout(
            "compressor_studio_clean",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn compressor_studio_clean_build_stereo_fails() {
        let params = default_params_for("compressor_studio_clean");
        let result = build_dynamics_processor_for_layout(
            "compressor_studio_clean",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(result.is_err());
    }

    #[test]
    fn compressor_studio_clean_process_silence_no_nan() {
        let params = default_params_for("compressor_studio_clean");
        let mut proc = build_dynamics_processor_for_layout(
            "compressor_studio_clean",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
    }

    #[test]
    fn compressor_studio_clean_via_compressor_api() {
        let schema = compressor_model_schema("compressor_studio_clean").expect("schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults");
        let proc = build_compressor_processor_for_layout(
            "compressor_studio_clean",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
    }

    // ── Gate: Basic Noise Gate ──────────────────────────────────────

    #[test]
    fn gate_basic_schema_has_expected_params() {
        let schema = dynamics_model_schema("gate_basic").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert_eq!(schema.model, "gate_basic");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"threshold"));
        assert!(param_names.contains(&"attack_ms"));
        assert!(param_names.contains(&"release_ms"));
    }

    #[test]
    fn gate_basic_defaults_normalize() {
        let schema = dynamics_model_schema("gate_basic").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok());
    }

    #[test]
    fn gate_basic_rejects_out_of_range() {
        let schema = dynamics_model_schema("gate_basic").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("attack_ms", ParameterValue::Float(999.0)); // max is 100
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn gate_basic_build_mono() {
        let params = default_params_for("gate_basic");
        let proc = build_dynamics_processor_for_layout(
            "gate_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn gate_basic_build_stereo_fails() {
        let params = default_params_for("gate_basic");
        let result = build_dynamics_processor_for_layout(
            "gate_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(result.is_err());
    }

    #[test]
    fn gate_basic_process_silence_no_nan() {
        let params = default_params_for("gate_basic");
        let mut proc = build_dynamics_processor_for_layout(
            "gate_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
    }

    #[test]
    fn gate_basic_silence_stays_silent() {
        let params = default_params_for("gate_basic");
        let mut proc = build_dynamics_processor_for_layout(
            "gate_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        // Gate should not add energy to silence
        assert!(
            output.iter().all(|s| s.abs() < 1e-6),
            "gate should not add energy to silence"
        );
    }

    #[test]
    fn gate_basic_via_gate_api() {
        let schema = gate_model_schema("gate_basic").expect("schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults");
        let proc = build_gate_processor_for_layout(
            "gate_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
    }

    // ── Limiter: Brick Wall ─────────────────────────────────────────

    #[test]
    fn limiter_brickwall_schema_has_expected_params() {
        let schema = dynamics_model_schema("limiter_brickwall").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert_eq!(schema.model, "limiter_brickwall");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"threshold"));
        assert!(param_names.contains(&"release_ms"));
        assert!(param_names.contains(&"ceiling"));
    }

    #[test]
    fn limiter_brickwall_defaults_normalize() {
        let schema = dynamics_model_schema("limiter_brickwall").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok());
    }

    #[test]
    fn limiter_brickwall_rejects_out_of_range() {
        let schema = dynamics_model_schema("limiter_brickwall").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("threshold", ParameterValue::Float(5.0)); // max is 0.0
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn limiter_brickwall_build_mono() {
        let params = default_params_for("limiter_brickwall");
        let proc = build_dynamics_processor_for_layout(
            "limiter_brickwall",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn limiter_brickwall_build_stereo_fails() {
        let params = default_params_for("limiter_brickwall");
        let result = build_dynamics_processor_for_layout(
            "limiter_brickwall",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(result.is_err());
    }

    #[test]
    fn limiter_brickwall_process_silence_no_nan() {
        let params = default_params_for("limiter_brickwall");
        let mut proc = build_dynamics_processor_for_layout(
            "limiter_brickwall",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
    }

    // ── Registry-level process tests for all native models ──────────

    fn native_dyn_models() -> Vec<&'static str> {
        supported_models()
            .iter()
            .copied()
            .filter(|m| dyn_type_label(m) == "NATIVE")
            .collect()
    }

    #[test]
    fn native_dyn_process_sine_mono_produces_finite() {
        for model in native_dyn_models() {
            let params = default_params_for(model);
            let mut proc = build_dynamics_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Mono,
            )
            .expect("build");
            match &mut proc {
                BlockProcessor::Mono(ref mut p) => {
                    for i in 0..1024 {
                        let input =
                            (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                        let out = p.process_sample(input);
                        assert!(
                            out.is_finite(),
                            "{model} mono produced non-finite at sample {i}: {out}"
                        );
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    #[test]
    fn native_dyn_process_block_1024_silence_all_finite() {
        for model in native_dyn_models() {
            let params = default_params_for(model);
            let mut proc = build_dynamics_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Mono,
            )
            .expect("build");
            let output = process_silence(&mut proc, 1024);
            assert!(
                output.iter().all(|s| s.is_finite()),
                "{model} block silence contains non-finite"
            );
        }
    }

    #[test]
    fn native_dyn_process_block_1024_sine_all_finite() {
        for model in native_dyn_models() {
            let params = default_params_for(model);
            let mut proc = build_dynamics_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Mono,
            )
            .expect("build");
            match &mut proc {
                BlockProcessor::Mono(ref mut p) => {
                    let mut buf: Vec<f32> = (0..1024)
                        .map(|i| {
                            (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5
                        })
                        .collect();
                    p.process_block(&mut buf);
                    for (i, &s) in buf.iter().enumerate() {
                        assert!(
                            s.is_finite(),
                            "{model} block sine non-finite at {i}: {s}"
                        );
                    }
                }
                _ => panic!("{model} expected Mono processor"),
            }
        }
    }

    // ── Display name / brand / type label ───────────────────────────

    #[test]
    fn dyn_display_name_returns_correct_values() {
        assert_eq!(dyn_display_name("compressor_studio_clean"), "Studio Clean Compressor");
        assert_eq!(dyn_display_name("gate_basic"), "Noise Gate");
        assert_eq!(dyn_display_name("limiter_brickwall"), "Brick Wall Limiter");
    }

    #[test]
    fn dyn_type_label_native_models() {
        assert_eq!(dyn_type_label("compressor_studio_clean"), "NATIVE");
        assert_eq!(dyn_type_label("gate_basic"), "NATIVE");
        assert_eq!(dyn_type_label("limiter_brickwall"), "NATIVE");
    }

    #[test]
    fn dyn_display_name_unknown_returns_empty() {
        assert_eq!(dyn_display_name("nonexistent_model"), "");
    }

    #[test]
    fn dyn_type_label_unknown_returns_empty() {
        assert_eq!(dyn_type_label("nonexistent_model"), "");
    }

    #[test]
    fn dyn_model_visual_returns_some_for_native() {
        let visual = dyn_model_visual("compressor_studio_clean");
        assert!(visual.is_some());
        let v = visual.unwrap();
        assert_eq!(v.type_label, "NATIVE");
    }

    #[test]
    fn dyn_model_visual_returns_none_for_unknown() {
        assert!(dyn_model_visual("nonexistent_model").is_none());
    }

    // ── LV2 models: schema only (build requires plugin binaries) ────

    #[test]
    fn lv2_tap_deesser_schema_valid() {
        let schema = dynamics_model_schema("lv2_tap_deesser").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_tap_dynamics_schema_valid() {
        let schema = dynamics_model_schema("lv2_tap_dynamics").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_tap_limiter_schema_valid() {
        let schema = dynamics_model_schema("lv2_tap_limiter").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_zamcomp_schema_valid() {
        let schema = dynamics_model_schema("lv2_zamcomp").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_zamgate_schema_valid() {
        let schema = dynamics_model_schema("lv2_zamgate").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_zamulticomp_schema_valid() {
        let schema = dynamics_model_schema("lv2_zamulticomp").expect("schema");
        assert_eq!(schema.effect_type, "dynamics");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_schemas_defaults_normalize() {
        for model in &[
            "lv2_tap_deesser",
            "lv2_tap_dynamics",
            "lv2_tap_limiter",
            "lv2_zamcomp",
            "lv2_zamgate",
            "lv2_zamulticomp",
        ] {
            let schema = dynamics_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(result.is_ok(), "defaults for '{model}' should normalize");
        }
    }

}
