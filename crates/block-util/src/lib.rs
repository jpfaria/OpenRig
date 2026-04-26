//! Utility implementations.
mod processor;
mod registry;
pub mod pitch_yin;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData, StreamHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UtilBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn util_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            UtilBackendKind::Native => "NATIVE",
            UtilBackendKind::Nam => "NAM",
            UtilBackendKind::Ir => "IR",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn util_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn util_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn util_type_label(model: &str) -> &'static str {
    util_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn utility_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn util_stream_kind(model_id: &str) -> &'static str {
    registry::util_stream_kind(model_id)
}

pub fn build_utility_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: usize,
    layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{
        build_utility_processor_for_layout, supported_models, util_display_name,
        util_type_label, utility_model_schema, util_stream_kind,
    };
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

    // ── helpers ──────────────────────────────────────────────────────────

    fn defaults_for(model: &str) -> ParameterSet {
        let schema = utility_model_schema(model).expect("schema");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    // ── registry-wide tests ─────────────────────────────────────────────

    #[test]
    fn registry_schema_all_models_return_valid_schema() {
        for model in supported_models() {
            let schema = utility_model_schema(model)
                .unwrap_or_else(|e| panic!("schema() failed for '{model}': {e}"));
            assert_eq!(schema.model, *model, "schema.model mismatch for '{model}'");
            assert_eq!(schema.effect_type, "utility", "effect_type mismatch for '{model}'");
        }
    }

    #[test]
    fn registry_schema_defaults_normalize_for_all_models() {
        for model in supported_models() {
            let schema = utility_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(
                result.is_ok(),
                "defaults failed to normalize for '{model}': {}",
                result.unwrap_err()
            );
        }
    }

    #[test]
    fn registry_metadata_all_models_have_display_name() {
        for model in supported_models() {
            let name = util_display_name(model);
            assert!(!name.is_empty(), "display_name empty for '{model}'");
            let label = util_type_label(model);
            assert!(!label.is_empty(), "type_label empty for '{model}'");
        }
    }

    #[test]
    fn registry_stream_kind_all_models_return_known_value() {
        for model in supported_models() {
            let kind = util_stream_kind(model);
            assert!(
                kind == "stream" || kind == "spectrum" || kind.is_empty(),
                "unexpected stream_kind '{kind}' for '{model}'"
            );
        }
    }

    #[test]
    fn registry_build_native_models_mono() {
        for model in supported_models() {
            let schema = utility_model_schema(model).expect("schema");
            // Only build Mono for models that support it (DualMono audio_mode)
            if schema.audio_mode != ModelAudioMode::DualMono {
                continue;
            }
            let params = defaults_for(model);
            let (processor, _stream) = build_utility_processor_for_layout(
                model,
                &params,
                48_000,
                AudioChannelLayout::Mono,
            )
            .unwrap_or_else(|e| panic!("build(Mono) failed for '{model}': {e}"));
            assert!(
                matches!(processor, BlockProcessor::Mono(_)),
                "'{model}' Mono build should return Mono variant"
            );
        }
    }

    #[test]
    fn registry_build_native_models_mono_to_stereo() {
        for model in supported_models() {
            let schema = utility_model_schema(model).expect("schema");
            // MonoToStereo models build with any layout but always return Stereo
            if schema.audio_mode != ModelAudioMode::MonoToStereo {
                continue;
            }
            let params = defaults_for(model);
            let (processor, _stream) = build_utility_processor_for_layout(
                model,
                &params,
                48_000,
                AudioChannelLayout::Mono,
            )
            .unwrap_or_else(|e| panic!("build() failed for MonoToStereo '{model}': {e}"));
            assert!(
                matches!(processor, BlockProcessor::Stereo(_)),
                "MonoToStereo '{model}' build should return Stereo variant"
            );
        }
    }

    #[test]
    fn registry_process_native_mono_silence_produces_finite() {
        for model in supported_models() {
            let schema = utility_model_schema(model).expect("schema");
            if schema.audio_mode != ModelAudioMode::DualMono {
                continue;
            }
            let params = defaults_for(model);
            let (processor, _stream) = build_utility_processor_for_layout(
                model,
                &params,
                48_000,
                AudioChannelLayout::Mono,
            )
            .unwrap();
            let mut proc = match processor {
                BlockProcessor::Mono(p) => p,
                BlockProcessor::Stereo(_) => panic!("expected Mono for '{model}'"),
            };
            for i in 0..256 {
                let out = proc.process_sample(0.0);
                assert!(
                    out.is_finite(),
                    "mono '{model}' produced non-finite at sample {i}: {out}"
                );
            }
        }
    }

    #[test]
    fn registry_process_native_stereo_silence_produces_finite() {
        for model in supported_models() {
            let schema = utility_model_schema(model).expect("schema");
            if schema.audio_mode != ModelAudioMode::MonoToStereo {
                continue;
            }
            let params = defaults_for(model);
            let (processor, _stream) = build_utility_processor_for_layout(
                model,
                &params,
                48_000,
                AudioChannelLayout::Mono,
            )
            .unwrap();
            let mut proc = match processor {
                BlockProcessor::Stereo(p) => p,
                BlockProcessor::Mono(_) => panic!("expected Stereo for '{model}'"),
            };
            for i in 0..256 {
                let [l, r] = proc.process_frame([0.0, 0.0]);
                assert!(
                    l.is_finite() && r.is_finite(),
                    "stereo '{model}' produced non-finite at frame {i}: [{l}, {r}]"
                );
            }
        }
    }
}
