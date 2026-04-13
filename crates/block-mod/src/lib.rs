//! Modulation implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ModBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn mod_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            ModBackendKind::Native => "NATIVE",
            ModBackendKind::Nam => "NAM",
            ModBackendKind::Ir => "IR",
            ModBackendKind::Lv2 => "LV2",
            ModBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn mod_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn mod_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn mod_type_label(model: &str) -> &'static str {
    mod_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn modulation_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_modulation_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_modulation_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_modulation_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
    use domain::value_objects::ParameterValue;

    // ── helpers ──────────────────────────────────────────────────────

    fn default_params(model: &str) -> ParameterSet {
        let schema = modulation_model_schema(model).expect("schema should exist");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    fn process_mono(proc: &mut BlockProcessor, frames: usize) -> Vec<f32> {
        match proc {
            BlockProcessor::Mono(p) => (0..frames)
                .map(|i| {
                    let input = if i == 0 { 1.0 } else { 0.0 };
                    p.process_sample(input)
                })
                .collect(),
            BlockProcessor::Stereo(p) => (0..frames)
                .map(|i| {
                    let input = if i == 0 { 1.0 } else { 0.0 };
                    let out = p.process_frame([input, input]);
                    out[0]
                })
                .collect(),
        }
    }

    // ── supported_models ─────────────────────────────────────────────

    #[test]
    fn supported_models_is_not_empty() {
        assert!(!supported_models().is_empty());
    }

    #[test]
    fn supported_models_all_have_valid_schema() {
        for model in supported_models() {
            let schema = modulation_model_schema(model)
                .unwrap_or_else(|e| panic!("schema for '{}' failed: {}", model, e));
            assert_eq!(schema.effect_type, "modulation", "wrong effect_type for {}", model);
            assert_eq!(schema.model, *model, "schema.model mismatch for {}", model);
        }
    }

    #[test]
    fn supported_models_all_have_visual_data() {
        for model in supported_models() {
            let visual = mod_model_visual(model);
            assert!(visual.is_some(), "missing visual data for {}", model);
        }
    }

    // ── classic_chorus ───────────────────────────────────────────────

    #[test]
    fn classic_chorus_schema_has_expected_params() {
        let schema = modulation_model_schema("classic_chorus").unwrap();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"));
        assert!(paths.contains(&"depth"));
        assert!(paths.contains(&"mix"));
        assert_eq!(schema.audio_mode, ModelAudioMode::MonoToStereo);
    }

    #[test]
    fn classic_chorus_schema_defaults_normalize() {
        let _ = default_params("classic_chorus");
    }

    #[test]
    fn classic_chorus_validate_rejects_out_of_range() {
        let schema = modulation_model_schema("classic_chorus").unwrap();
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(999.0));
        ps.insert("depth", ParameterValue::Float(50.0));
        ps.insert("mix", ParameterValue::Float(50.0));
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn classic_chorus_build_mono() {
        let params = default_params("classic_chorus");
        let proc = build_modulation_processor_for_layout(
            "classic_chorus", &params, 48000.0, AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn classic_chorus_build_stereo() {
        let params = default_params("classic_chorus");
        let proc = build_modulation_processor_for_layout(
            "classic_chorus", &params, 48000.0, AudioChannelLayout::Stereo,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
    }

    #[test]
    fn classic_chorus_process_produces_non_nan() {
        let params = default_params("classic_chorus");
        let mut proc = build_modulation_processor_for_layout(
            "classic_chorus", &params, 48000.0, AudioChannelLayout::Mono,
        ).unwrap();
        let output = process_mono(&mut proc, 256);
        for (i, s) in output.iter().enumerate() {
            assert!(!s.is_nan(), "NaN at frame {} for classic_chorus mono", i);
        }
    }

    #[test]
    fn classic_chorus_process_stereo_produces_non_nan() {
        let params = default_params("classic_chorus");
        let mut proc = build_modulation_processor_for_layout(
            "classic_chorus", &params, 48000.0, AudioChannelLayout::Stereo,
        ).unwrap();
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

    // ── ensemble_chorus ──────────────────────────────────────────────

    #[test]
    fn ensemble_chorus_schema_has_expected_params() {
        let schema = modulation_model_schema("ensemble_chorus").unwrap();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"));
        assert!(paths.contains(&"depth"));
        assert!(paths.contains(&"mix"));
        assert_eq!(schema.audio_mode, ModelAudioMode::MonoToStereo);
    }

    #[test]
    fn ensemble_chorus_schema_defaults_normalize() {
        let _ = default_params("ensemble_chorus");
    }

    #[test]
    fn ensemble_chorus_validate_rejects_out_of_range() {
        let schema = modulation_model_schema("ensemble_chorus").unwrap();
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(0.5));
        ps.insert("depth", ParameterValue::Float(200.0)); // out of range
        ps.insert("mix", ParameterValue::Float(50.0));
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn ensemble_chorus_build_mono() {
        let params = default_params("ensemble_chorus");
        let proc = build_modulation_processor_for_layout(
            "ensemble_chorus", &params, 48000.0, AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn ensemble_chorus_build_stereo() {
        let params = default_params("ensemble_chorus");
        let proc = build_modulation_processor_for_layout(
            "ensemble_chorus", &params, 48000.0, AudioChannelLayout::Stereo,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
    }

    #[test]
    fn ensemble_chorus_process_produces_non_nan() {
        let params = default_params("ensemble_chorus");
        let mut proc = build_modulation_processor_for_layout(
            "ensemble_chorus", &params, 48000.0, AudioChannelLayout::Mono,
        ).unwrap();
        let output = process_mono(&mut proc, 256);
        for (i, s) in output.iter().enumerate() {
            assert!(!s.is_nan(), "NaN at frame {} for ensemble_chorus", i);
        }
    }

    // ── stereo_chorus ────────────────────────────────────────────────

    #[test]
    fn stereo_chorus_schema_has_expected_params() {
        let schema = modulation_model_schema("stereo_chorus").unwrap();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"));
        assert!(paths.contains(&"depth"));
        assert!(paths.contains(&"mix"));
        assert!(paths.contains(&"spread"));
        assert_eq!(schema.audio_mode, ModelAudioMode::MonoToStereo);
    }

    #[test]
    fn stereo_chorus_schema_defaults_normalize() {
        let _ = default_params("stereo_chorus");
    }

    #[test]
    fn stereo_chorus_validate_rejects_out_of_range() {
        let schema = modulation_model_schema("stereo_chorus").unwrap();
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(0.5));
        ps.insert("depth", ParameterValue::Float(50.0));
        ps.insert("mix", ParameterValue::Float(50.0));
        ps.insert("spread", ParameterValue::Float(150.0)); // out of range
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn stereo_chorus_build_mono() {
        let params = default_params("stereo_chorus");
        let proc = build_modulation_processor_for_layout(
            "stereo_chorus", &params, 48000.0, AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        // stereo_chorus always returns Stereo
        assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
    }

    #[test]
    fn stereo_chorus_build_stereo() {
        let params = default_params("stereo_chorus");
        let proc = build_modulation_processor_for_layout(
            "stereo_chorus", &params, 48000.0, AudioChannelLayout::Stereo,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
    }

    #[test]
    fn stereo_chorus_process_produces_non_nan() {
        let params = default_params("stereo_chorus");
        let mut proc = build_modulation_processor_for_layout(
            "stereo_chorus", &params, 48000.0, AudioChannelLayout::Stereo,
        ).unwrap();
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

    // ── tremolo_sine ─────────────────────────────────────────────────

    #[test]
    fn tremolo_sine_schema_has_expected_params() {
        let schema = modulation_model_schema("tremolo_sine").unwrap();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"));
        assert!(paths.contains(&"depth"));
        assert_eq!(schema.audio_mode, ModelAudioMode::MonoToStereo);
    }

    #[test]
    fn tremolo_sine_schema_defaults_normalize() {
        let _ = default_params("tremolo_sine");
    }

    #[test]
    fn tremolo_sine_validate_rejects_out_of_range() {
        let schema = modulation_model_schema("tremolo_sine").unwrap();
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(-1.0)); // out of range
        ps.insert("depth", ParameterValue::Float(50.0));
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn tremolo_sine_build_mono() {
        let params = default_params("tremolo_sine");
        let proc = build_modulation_processor_for_layout(
            "tremolo_sine", &params, 48000.0, AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn tremolo_sine_build_stereo() {
        let params = default_params("tremolo_sine");
        let proc = build_modulation_processor_for_layout(
            "tremolo_sine", &params, 48000.0, AudioChannelLayout::Stereo,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
    }

    #[test]
    fn tremolo_sine_process_produces_non_nan() {
        let params = default_params("tremolo_sine");
        let mut proc = build_modulation_processor_for_layout(
            "tremolo_sine", &params, 48000.0, AudioChannelLayout::Mono,
        ).unwrap();
        let output = process_mono(&mut proc, 256);
        for (i, s) in output.iter().enumerate() {
            assert!(!s.is_nan(), "NaN at frame {} for tremolo_sine", i);
        }
    }

    #[test]
    fn tremolo_sine_output_bounded_by_input() {
        let params = default_params("tremolo_sine");
        let mut proc = build_modulation_processor_for_layout(
            "tremolo_sine", &params, 48000.0, AudioChannelLayout::Mono,
        ).unwrap();
        match &mut proc {
            BlockProcessor::Mono(p) => {
                for _ in 0..512 {
                    let out = p.process_sample(1.0);
                    assert!(out <= 1.0 && out >= 0.0,
                        "tremolo output {} should be in [0,1] for unit input", out);
                }
            }
            _ => panic!("expected Mono processor"),
        }
    }

    // ── vibrato ──────────────────────────────────────────────────────

    #[test]
    fn vibrato_schema_has_expected_params() {
        let schema = modulation_model_schema("vibrato").unwrap();
        let paths: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(paths.contains(&"rate_hz"));
        assert!(paths.contains(&"depth"));
        assert_eq!(schema.audio_mode, ModelAudioMode::MonoToStereo);
    }

    #[test]
    fn vibrato_schema_defaults_normalize() {
        let _ = default_params("vibrato");
    }

    #[test]
    fn vibrato_validate_rejects_out_of_range() {
        let schema = modulation_model_schema("vibrato").unwrap();
        let mut ps = ParameterSet::default();
        ps.insert("rate_hz", ParameterValue::Float(0.5));
        ps.insert("depth", ParameterValue::Float(200.0)); // out of range
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn vibrato_build_mono() {
        let params = default_params("vibrato");
        let proc = build_modulation_processor_for_layout(
            "vibrato", &params, 48000.0, AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn vibrato_build_stereo() {
        let params = default_params("vibrato");
        let proc = build_modulation_processor_for_layout(
            "vibrato", &params, 48000.0, AudioChannelLayout::Stereo,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Stereo(_)));
    }

    #[test]
    fn vibrato_process_produces_non_nan() {
        let params = default_params("vibrato");
        let mut proc = build_modulation_processor_for_layout(
            "vibrato", &params, 48000.0, AudioChannelLayout::Mono,
        ).unwrap();
        let output = process_mono(&mut proc, 256);
        for (i, s) in output.iter().enumerate() {
            assert!(!s.is_nan(), "NaN at frame {} for vibrato", i);
        }
    }

    // ── display_name / brand / type_label helpers ────────────────────

    #[test]
    fn classic_chorus_display_name_matches() {
        assert_eq!(mod_display_name("classic_chorus"), "Classic Chorus");
    }

    #[test]
    fn ensemble_chorus_display_name_matches() {
        assert_eq!(mod_display_name("ensemble_chorus"), "Ensemble Chorus");
    }

    #[test]
    fn stereo_chorus_display_name_matches() {
        assert_eq!(mod_display_name("stereo_chorus"), "Stereo Chorus");
    }

    #[test]
    fn tremolo_sine_display_name_matches() {
        assert_eq!(mod_display_name("tremolo_sine"), "Sine Tremolo");
    }

    #[test]
    fn vibrato_display_name_matches() {
        assert_eq!(mod_display_name("vibrato"), "Vibrato");
    }

    #[test]
    fn native_models_type_label_is_native() {
        for model in &["classic_chorus", "ensemble_chorus", "stereo_chorus", "tremolo_sine", "vibrato"] {
            assert_eq!(mod_type_label(model), "NATIVE", "wrong type_label for {}", model);
        }
    }

    #[test]
    fn unknown_model_returns_empty_strings() {
        assert_eq!(mod_display_name("nonexistent"), "");
        assert_eq!(mod_brand("nonexistent"), "");
        assert_eq!(mod_type_label("nonexistent"), "");
    }
}
