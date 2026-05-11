use super::{
    build_utility_processor_for_layout, supported_models, util_display_name, util_stream_kind,
    util_type_label, utility_model_schema,
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
        assert_eq!(
            schema.effect_type, "utility",
            "effect_type mismatch for '{model}'"
        );
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
        let (processor, _stream) =
            build_utility_processor_for_layout(model, &params, 48_000, AudioChannelLayout::Mono)
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
        let (processor, _stream) =
            build_utility_processor_for_layout(model, &params, 48_000, AudioChannelLayout::Mono)
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
        let (processor, _stream) =
            build_utility_processor_for_layout(model, &params, 48_000, AudioChannelLayout::Mono)
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
        let (processor, _stream) =
            build_utility_processor_for_layout(model, &params, 48_000, AudioChannelLayout::Mono)
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
