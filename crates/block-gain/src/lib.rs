//! Gain blocks such as boost, overdrive, distortion, and fuzz.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GainBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
    Vst3,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn gain_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            GainBackendKind::Native => "NATIVE",
            GainBackendKind::Nam => "NAM",
            GainBackendKind::Ir => "IR",
            GainBackendKind::Lv2 => "LV2",
            GainBackendKind::Vst3 => "VST3",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
        thumbnail_path: gain_thumbnail(model_id),
        available: registry::is_model_available(model_id),
    })
}

pub fn gain_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn gain_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn gain_type_label(model: &str) -> &'static str {
    gain_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn gain_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn gain_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_gain_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_gain_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_gain_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gain_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{
        build_gain_processor_for_layout, gain_brand, gain_display_name, gain_model_schema,
        gain_type_label, supported_models, validate_gain_params,
    };
    use crate::registry::find_model_definition;
    use crate::GainBackendKind;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
    use domain::value_objects::ParameterValue;

    // ── helpers ──────────────────────────────────────────────────────────

    fn is_native(model: &str) -> bool {
        find_model_definition(model)
            .map(|d| d.backend_kind == GainBackendKind::Native)
            .unwrap_or(false)
    }

    fn defaults_for(model: &str) -> ParameterSet {
        let schema = gain_model_schema(model).expect("schema");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    // ── registry-wide tests ─────────────────────────────────────────────

    #[test]
    fn registry_schema_all_models_return_non_empty_schema() {
        for model in supported_models() {
            let schema = gain_model_schema(model)
                .unwrap_or_else(|e| panic!("schema() failed for '{model}': {e}"));
            assert_eq!(schema.model, *model, "schema.model mismatch for '{model}'");
            assert_eq!(
                schema.effect_type, "gain",
                "effect_type mismatch for '{model}'"
            );
            assert!(
                !schema.parameters.is_empty(),
                "model '{model}' should expose at least one parameter"
            );
        }
    }

    #[test]
    fn registry_validate_all_models_accept_defaults() {
        for model in supported_models() {
            let params = defaults_for(model);
            validate_gain_params(model, &params)
                .unwrap_or_else(|e| panic!("validate() rejected defaults for '{model}': {e}"));
        }
    }

    #[test]
    fn registry_metadata_all_models_have_display_name_and_brand() {
        for model in supported_models() {
            let name = gain_display_name(model);
            assert!(!name.is_empty(), "display_name empty for '{model}'");
            let brand = gain_brand(model);
            assert!(!brand.is_empty(), "brand empty for '{model}'");
            let label = gain_type_label(model);
            assert!(!label.is_empty(), "type_label empty for '{model}'");
        }
    }

    #[test]
    fn registry_schema_defaults_normalize_for_all_models() {
        for model in supported_models() {
            let schema = gain_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(
                result.is_ok(),
                "defaults failed to normalize for '{model}': {}",
                result.unwrap_err()
            );
        }
    }

    #[test]
    fn registry_build_native_models_mono() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let processor =
                build_gain_processor_for_layout(model, &params, 48_000.0, AudioChannelLayout::Mono)
                    .unwrap_or_else(|e| panic!("build(Mono) failed for native '{model}': {e}"));
            assert!(
                matches!(processor, BlockProcessor::Mono(_)),
                "native '{model}' Mono build should return Mono variant"
            );
        }
    }

    #[test]
    fn registry_build_native_models_stereo() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            // Build must succeed; variant may be Mono (DualMono) or Stereo
            let _processor = build_gain_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            )
            .unwrap_or_else(|e| panic!("build(Stereo) failed for native '{model}': {e}"));
        }
    }

    #[test]
    fn registry_process_native_mono_silence_produces_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_gain_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            )
            .unwrap()
            {
                BlockProcessor::Mono(p) => p,
                BlockProcessor::Stereo(_) => panic!("expected Mono for '{model}'"),
            };
            for i in 0..256 {
                let out = proc.process_sample(0.0);
                assert!(
                    out.is_finite(),
                    "native mono '{model}' produced non-finite at sample {i}: {out}"
                );
            }
        }
    }

    #[test]
    fn registry_process_native_stereo_silence_produces_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let processor = build_gain_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            )
            .unwrap();
            match processor {
                BlockProcessor::Stereo(mut p) => {
                    for i in 0..256 {
                        let [l, r] = p.process_frame([0.0, 0.0]);
                        assert!(
                            l.is_finite() && r.is_finite(),
                            "native stereo '{model}' produced non-finite at frame {i}: [{l}, {r}]"
                        );
                    }
                }
                BlockProcessor::Mono(mut p) => {
                    // DualMono models return Mono; engine wraps to stereo
                    for i in 0..256 {
                        let out = p.process_sample(0.0);
                        assert!(
                            out.is_finite(),
                            "native dualmono '{model}' produced non-finite at sample {i}: {out}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn registry_process_native_mono_signal_produces_non_nan() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_gain_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            )
            .unwrap()
            {
                BlockProcessor::Mono(p) => p,
                BlockProcessor::Stereo(_) => panic!("expected Mono for '{model}'"),
            };
            // Feed a simple sine-ish ramp to exercise the DSP
            for i in 0..512 {
                let input = (i as f32 / 512.0 * std::f32::consts::TAU).sin() * 0.5;
                let out = proc.process_sample(input);
                assert!(
                    !out.is_nan(),
                    "native mono '{model}' produced NaN at sample {i}"
                );
            }
        }
    }

    // ── non-native models: build requires external assets, skip ──────

    #[test]
    #[ignore]
    fn registry_build_non_native_models_ignored() {
        for model in supported_models().iter().filter(|m| !is_native(m)) {
            let params = defaults_for(model);
            let _ =
                build_gain_processor_for_layout(model, &params, 48_000.0, AudioChannelLayout::Mono);
        }
    }

    // ── registry: process 1024 frames at 44100Hz ─────────────────────

    #[test]
    fn registry_process_native_mono_44100hz_1024_frames_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_gain_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Mono,
            )
            .unwrap()
            {
                BlockProcessor::Mono(p) => p,
                BlockProcessor::Stereo(_) => panic!("expected Mono for '{model}'"),
            };
            for i in 0..1024 {
                let input = (i as f32 / 1024.0 * std::f32::consts::TAU * 2.0).sin() * 0.3;
                let out = proc.process_sample(input);
                assert!(
                    out.is_finite(),
                    "native mono '{model}' @44100 produced non-finite at sample {i}: {out}"
                );
            }
        }
    }

    #[test]
    fn registry_process_native_stereo_44100hz_1024_frames_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let processor = build_gain_processor_for_layout(
                model,
                &params,
                44_100.0,
                AudioChannelLayout::Stereo,
            )
            .unwrap();
            match processor {
                BlockProcessor::Stereo(mut p) => {
                    for i in 0..1024 {
                        let input = (i as f32 / 1024.0 * std::f32::consts::TAU * 2.0).sin() * 0.3;
                        let [l, r] = p.process_frame([input, input]);
                        assert!(
                            l.is_finite() && r.is_finite(),
                            "native stereo '{model}' @44100 non-finite at {i}: [{l}, {r}]"
                        );
                    }
                }
                BlockProcessor::Mono(mut p) => {
                    for i in 0..1024 {
                        let input = (i as f32 / 1024.0 * std::f32::consts::TAU * 2.0).sin() * 0.3;
                        let out = p.process_sample(input);
                        assert!(
                            out.is_finite(),
                            "native dualmono '{model}' @44100 non-finite at {i}: {out}"
                        );
                    }
                }
            }
        }
    }

    // ── registry: asset_summary for native models ───────────────────

    #[test]
    fn registry_asset_summary_native_models_return_nonempty() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let summary = super::gain_asset_summary(model, &params)
                .unwrap_or_else(|e| panic!("asset_summary failed for '{model}': {e}"));
            assert!(
                !summary.is_empty(),
                "asset_summary should be non-empty for '{model}'"
            );
        }
    }

    // ── registry: visual data for all models ────────────────────────

    #[test]
    fn registry_visual_data_all_models_have_entries() {
        for model in supported_models() {
            let visual = super::gain_model_visual(model);
            assert!(
                visual.is_some(),
                "gain_model_visual should return Some for '{model}'"
            );
            let v = visual.unwrap();
            assert!(
                !v.brand.is_empty(),
                "brand should be non-empty for '{model}'"
            );
            assert!(
                !v.type_label.is_empty(),
                "type_label should be non-empty for '{model}'"
            );
            assert!(
                !v.supported_instruments.is_empty(),
                "supported_instruments should be non-empty for '{model}'"
            );
        }
    }

    // ── edge cases: unknown model ───────────────────────────────────

    #[test]
    fn unknown_model_returns_empty_name_and_brand() {
        assert_eq!(gain_display_name("nonexistent_model"), "");
        assert_eq!(gain_brand("nonexistent_model"), "");
        assert_eq!(gain_type_label("nonexistent_model"), "");
    }

    #[test]
    fn unknown_model_schema_fails() {
        assert!(super::gain_model_schema("nonexistent_model").is_err());
    }

    #[test]
    fn unknown_model_build_fails() {
        let params = ParameterSet::default();
        assert!(build_gain_processor_for_layout(
            "nonexistent_model",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .is_err());
    }

    #[test]
    fn unknown_model_validate_fails() {
        let params = ParameterSet::default();
        assert!(super::validate_gain_params("nonexistent_model", &params).is_err());
    }

    #[test]
    fn unknown_model_visual_returns_none() {
        assert!(super::gain_model_visual("nonexistent_model").is_none());
    }

    // ── existing specific tests (kept) ──────────────────────────────────

    #[test]
    fn ibanez_ts9_schema_exposes_drive_tone_and_level() {
        let schema = gain_model_schema("ibanez_ts9").expect("ts9 schema should exist");

        assert_eq!(schema.effect_type, "gain");
        assert_eq!(schema.model, "ibanez_ts9");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        assert_eq!(
            schema
                .parameters
                .iter()
                .map(|parameter| parameter.path.as_str())
                .collect::<Vec<_>>(),
            vec!["drive", "tone", "level"]
        );
    }

    #[test]
    fn ibanez_ts9_builds_for_mono_and_stereo_layouts() {
        let schema = gain_model_schema("ibanez_ts9").expect("ts9 schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");

        let mono = build_gain_processor_for_layout(
            "ibanez_ts9",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("mono ts9 should build");
        assert!(matches!(mono, BlockProcessor::Mono(_)));

        let stereo = build_gain_processor_for_layout(
            "ibanez_ts9",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        )
        .expect("stereo ts9 should build");
        assert!(matches!(stereo, BlockProcessor::Stereo(_)));
    }

    #[test]
    fn ibanez_ts9_level_changes_output_gain() {
        let schema = gain_model_schema("ibanez_ts9").expect("ts9 schema should exist");

        let mut quiet = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        quiet.insert("drive", ParameterValue::Float(35.0));
        quiet.insert("tone", ParameterValue::Float(50.0));
        quiet.insert("level", ParameterValue::Float(20.0));

        let mut loud = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        loud.insert("drive", ParameterValue::Float(35.0));
        loud.insert("tone", ParameterValue::Float(50.0));
        loud.insert("level", ParameterValue::Float(80.0));

        let mut quiet_processor = match build_gain_processor_for_layout(
            "ibanez_ts9",
            &quiet,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("quiet ts9 should build")
        {
            BlockProcessor::Mono(processor) => processor,
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        };

        let mut loud_processor = match build_gain_processor_for_layout(
            "ibanez_ts9",
            &loud,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("loud ts9 should build")
        {
            BlockProcessor::Mono(processor) => processor,
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        };

        let quiet_output = quiet_processor.process_sample(0.2).abs();
        let loud_output = loud_processor.process_sample(0.2).abs();

        assert!(
            loud_output > quiet_output,
            "level should raise output: quiet={quiet_output}, loud={loud_output}"
        );
    }
}

pub fn is_gain_model_available(model: &str) -> bool {
    registry::is_model_available(model)
}

/// Returns the catalog thumbnail path (relative to project root) for a model,
/// or `None` if the model has no thumbnail registered.
pub fn gain_thumbnail(model: &str) -> Option<&'static str> {
    registry::THUMBNAILS
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, path)| *path)
}
