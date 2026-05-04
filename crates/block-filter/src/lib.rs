//! Filter implementations.
pub mod model_visual;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FilterBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn filter_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            FilterBackendKind::Native => "NATIVE",
            FilterBackendKind::Nam => "NAM",
            FilterBackendKind::Ir => "IR",
            FilterBackendKind::Lv2 => "LV2",
            FilterBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn filter_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn filter_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn filter_type_label(model: &str) -> &'static str {
    filter_model_visual(model)
        .map(|v| v.type_label)
        .unwrap_or("")
}

pub fn filter_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_filter_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_filter_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_filter_processor_for_layout(
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

    // ── Helper ──────────────────────────────────────────────────────

    fn default_params_for(model: &str) -> ParameterSet {
        let schema = filter_model_schema(model).expect("schema should exist");
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
    fn supported_filter_models_expose_schema() {
        for model in supported_models() {
            assert!(
                filter_model_schema(model).is_ok(),
                "expected '{model}' to have a valid schema"
            );
        }
    }

    #[test]
    fn supported_filter_models_defaults_normalize() {
        for model in supported_models() {
            let schema = filter_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(result.is_ok(), "defaults for '{model}' should normalize");
        }
    }

    // ── Three Band EQ ──────────────────────────────────────────────

    #[test]
    fn eq_three_band_basic_schema_has_expected_params() {
        let schema = filter_model_schema("eq_three_band_basic").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert_eq!(schema.model, "eq_three_band_basic");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"low_gain"));
        assert!(param_names.contains(&"low_freq"));
        assert!(param_names.contains(&"mid_gain"));
        assert!(param_names.contains(&"mid_freq"));
        assert!(param_names.contains(&"mid_q"));
        assert!(param_names.contains(&"high_gain"));
        assert!(param_names.contains(&"high_freq"));
    }

    #[test]
    fn eq_three_band_basic_defaults_normalize() {
        let schema = filter_model_schema("eq_three_band_basic").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok());
    }

    #[test]
    fn eq_three_band_basic_rejects_out_of_range() {
        let schema = filter_model_schema("eq_three_band_basic").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("low_gain", ParameterValue::Float(50.0)); // max is 12.0
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn eq_three_band_basic_build_mono() {
        let params = default_params_for("eq_three_band_basic");
        let proc = build_filter_processor_for_layout(
            "eq_three_band_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn eq_three_band_basic_build_stereo_fails() {
        let params = default_params_for("eq_three_band_basic");
        let result = build_filter_processor_for_layout(
            "eq_three_band_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(result.is_err());
    }

    #[test]
    fn eq_three_band_basic_process_silence_no_nan() {
        let params = default_params_for("eq_three_band_basic");
        let mut proc = build_filter_processor_for_layout(
            "eq_three_band_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
    }

    #[test]
    fn eq_three_band_basic_flat_eq_passes_silence() {
        let params = default_params_for("eq_three_band_basic");
        let mut proc = build_filter_processor_for_layout(
            "eq_three_band_basic",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        assert!(
            output.iter().all(|s| s.abs() < 1e-6),
            "flat EQ should not add energy to silence"
        );
    }

    // ── Guitar EQ (4-band tone shaper, #303) ───────────────────────

    #[test]
    fn guitar_eq_schema_has_expected_params() {
        let schema = filter_model_schema("native_guitar_eq").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert_eq!(schema.model, "native_guitar_eq");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(param_names, vec!["low", "low_mid", "high_mid", "high"]);
    }

    #[test]
    fn guitar_eq_defaults_normalize() {
        let schema = filter_model_schema("native_guitar_eq").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok());
    }

    #[test]
    fn guitar_eq_rejects_out_of_range() {
        let schema = filter_model_schema("native_guitar_eq").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("low", ParameterValue::Float(50.0)); // max is +12 dB
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn guitar_eq_build_mono() {
        let params = default_params_for("native_guitar_eq");
        let proc = build_filter_processor_for_layout(
            "native_guitar_eq",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn guitar_eq_build_stereo_fails() {
        let params = default_params_for("native_guitar_eq");
        let result = build_filter_processor_for_layout(
            "native_guitar_eq",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(result.is_err());
    }

    // ── Guitar HPF/LPF (formerly "Guitar EQ", renamed in #303) ─────

    #[test]
    fn guitar_hpf_lpf_schema_has_expected_params() {
        let schema = filter_model_schema("native_guitar_hpf_lpf").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert_eq!(schema.model, "native_guitar_hpf_lpf");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        let param_names: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
        assert!(param_names.contains(&"low_cut"));
        assert!(param_names.contains(&"high_cut"));
    }

    #[test]
    fn guitar_hpf_lpf_defaults_normalize() {
        let schema = filter_model_schema("native_guitar_hpf_lpf").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok());
    }

    #[test]
    fn guitar_hpf_lpf_rejects_out_of_range() {
        let schema = filter_model_schema("native_guitar_hpf_lpf").expect("schema");
        let mut ps = ParameterSet::default();
        ps.insert("low_cut", ParameterValue::Float(200.0)); // max is 100
        assert!(ps.normalized_against(&schema).is_err());
    }

    #[test]
    fn guitar_hpf_lpf_build_mono() {
        let params = default_params_for("native_guitar_hpf_lpf");
        let proc = build_filter_processor_for_layout(
            "native_guitar_hpf_lpf",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        );
        assert!(proc.is_ok());
        assert!(matches!(proc.unwrap(), BlockProcessor::Mono(_)));
    }

    #[test]
    fn guitar_hpf_lpf_build_stereo_fails() {
        let params = default_params_for("native_guitar_hpf_lpf");
        let result = build_filter_processor_for_layout(
            "native_guitar_hpf_lpf",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(result.is_err());
    }

    #[test]
    fn guitar_eq_process_silence_no_nan() {
        let params = default_params_for("native_guitar_eq");
        let mut proc = build_filter_processor_for_layout(
            "native_guitar_eq",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("build");
        let output = process_silence(&mut proc, 256);
        assert!(output.iter().all(|s| !s.is_nan()), "output contains NaN");
    }

    // ── Display name / brand / type label ───────────────────────────

    #[test]
    fn filter_display_name_returns_correct_values() {
        assert_eq!(filter_display_name("eq_three_band_basic"), "Three Band EQ");
        assert_eq!(filter_display_name("native_guitar_eq"), "Guitar EQ");
    }

    #[test]
    fn filter_type_label_native_models() {
        assert_eq!(filter_type_label("eq_three_band_basic"), "NATIVE");
        assert_eq!(filter_type_label("native_guitar_eq"), "NATIVE");
    }

    #[test]
    fn filter_display_name_unknown_returns_empty() {
        assert_eq!(filter_display_name("nonexistent_model"), "");
    }

    #[test]
    fn filter_type_label_unknown_returns_empty() {
        assert_eq!(filter_type_label("nonexistent_model"), "");
    }

    #[test]
    fn filter_model_visual_returns_some_for_native() {
        let visual = filter_model_visual("eq_three_band_basic");
        assert!(visual.is_some());
        let v = visual.unwrap();
        assert_eq!(v.type_label, "NATIVE");
    }

    #[test]
    fn filter_model_visual_returns_none_for_unknown() {
        assert!(filter_model_visual("nonexistent_model").is_none());
    }

    // ── native model helpers ──────────────────────────────────────────

    fn native_filter_models() -> Vec<&'static str> {
        supported_models()
            .iter()
            .copied()
            .filter(|m| filter_type_label(m) == "NATIVE")
            .collect()
    }

    // ── native process tests (registry-level) ─────────────────────────

    #[test]
    fn native_filter_build_mono_at_44100() {
        for model in native_filter_models() {
            let params = default_params_for(model);
            let result = build_filter_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Mono,
            );
            assert!(
                result.is_ok(),
                "{model} should build mono at 44100Hz: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn native_filter_process_silence_mono_all_finite() {
        for model in native_filter_models() {
            let params = default_params_for(model);
            let mut proc = build_filter_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Mono,
            )
            .expect("build");
            let output = process_silence(&mut proc, 1024);
            for (i, s) in output.iter().enumerate() {
                assert!(s.is_finite(), "{model} mono not finite at sample {i}");
            }
        }
    }

    fn process_sine(processor: &mut BlockProcessor, frames: usize, sample_rate: f32) -> Vec<f32> {
        match processor {
            BlockProcessor::Mono(p) => {
                let mut buf: Vec<f32> = (0..frames)
                    .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
                    .collect();
                p.process_block(&mut buf);
                buf
            }
            BlockProcessor::Stereo(p) => {
                let mut buf: Vec<[f32; 2]> = (0..frames)
                    .map(|i| {
                        let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin();
                        [s, s]
                    })
                    .collect();
                p.process_block(&mut buf);
                buf.iter().flat_map(|pair| pair.iter().copied()).collect()
            }
        }
    }

    #[test]
    fn native_filter_process_sine_mono_all_finite_and_nonzero() {
        for model in native_filter_models() {
            let params = default_params_for(model);
            let mut proc = build_filter_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Mono,
            )
            .expect("build");
            let output = process_sine(&mut proc, 1024, 44_100.0);
            let any_nonzero = output.iter().any(|s| s.abs() > 1e-10);
            for (i, s) in output.iter().enumerate() {
                assert!(s.is_finite(), "{model} mono sine not finite at frame {i}");
            }
            assert!(
                any_nonzero,
                "{model} mono produced all zeros for sine input"
            );
        }
    }

    #[test]
    fn native_filter_process_block_mono_all_finite() {
        for model in native_filter_models() {
            let params = default_params_for(model);
            let mut proc = build_filter_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Mono,
            )
            .expect("build");
            let output = process_sine(&mut proc, 1024, 44_100.0);
            for (i, s) in output.iter().enumerate() {
                assert!(s.is_finite(), "{model} mono block not finite at frame {i}");
            }
        }
    }

    #[test]
    fn native_filter_stereo_build_rejected() {
        // Both native filter models are mono-only
        for model in native_filter_models() {
            let params = default_params_for(model);
            let result = build_filter_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Stereo,
            );
            assert!(result.is_err(), "{model} should reject stereo layout");
        }
    }

    // ── LV2 models: schema only (build requires plugin binaries) ────

    #[test]
    fn lv2_tap_equalizer_schema_valid() {
        let schema = filter_model_schema("lv2_tap_equalizer").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_tap_equalizer_bw_schema_valid() {
        let schema = filter_model_schema("lv2_tap_equalizer_bw").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_zameq2_schema_valid() {
        let schema = filter_model_schema("lv2_zameq2").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_zamgeq31_schema_valid() {
        let schema = filter_model_schema("lv2_zamgeq31").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_caps_autofilter_schema_valid() {
        let schema = filter_model_schema("lv2_caps_autofilter").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_fomp_autowah_schema_valid() {
        let schema = filter_model_schema("lv2_fomp_autowah").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_mod_hpf_schema_valid() {
        let schema = filter_model_schema("lv2_mod_hpf").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_mod_lpf_schema_valid() {
        let schema = filter_model_schema("lv2_mod_lpf").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_artyfx_filta_schema_valid() {
        let schema = filter_model_schema("lv2_artyfx_filta").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn lv2_mud_schema_valid() {
        let schema = filter_model_schema("lv2_mud").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    // ── VST3 model: schema only ─────────────────────────────────────

    #[test]
    fn vst3_modeq_schema_valid() {
        let schema = filter_model_schema("vst3_modeq").expect("schema");
        assert_eq!(schema.effect_type, "filter");
        assert!(!schema.parameters.is_empty());
    }

    #[test]
    fn vst3_modeq_defaults_normalize() {
        let schema = filter_model_schema("vst3_modeq").expect("schema");
        let result = ParameterSet::default().normalized_against(&schema);
        assert!(result.is_ok());
    }
}
