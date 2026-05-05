    use super::{
        build_preamp_processor_for_layout, preamp_model_schema, supported_models,
        validate_preamp_params, preamp_display_name, preamp_brand, preamp_type_label,
    };
    use crate::registry::find_model_definition;
    use crate::PreampBackendKind;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor};

    // ── helpers ──────────────────────────────────────────────────────────

    fn is_native(model: &str) -> bool {
        find_model_definition(model)
            .map(|d| d.backend_kind == PreampBackendKind::Native)
            .unwrap_or(false)
    }

    fn defaults_for(model: &str) -> ParameterSet {
        let schema = preamp_model_schema(model).expect("schema");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    // ── registry-wide tests ─────────────────────────────────────────────

    #[test]
    fn registry_schema_all_models_return_non_empty_schema() {
        for model in supported_models() {
            let schema = preamp_model_schema(model)
                .unwrap_or_else(|e| panic!("schema() failed for '{model}': {e}"));
            assert_eq!(schema.model, *model, "schema.model mismatch for '{model}'");
            assert_eq!(schema.effect_type, "preamp", "effect_type mismatch for '{model}'");
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
            validate_preamp_params(model, &params)
                .unwrap_or_else(|e| panic!("validate() rejected defaults for '{model}': {e}"));
        }
    }

    #[test]
    fn registry_metadata_all_models_have_display_name_and_brand() {
        for model in supported_models() {
            let name = preamp_display_name(model).expect("display_name");
            assert!(!name.is_empty(), "display_name empty for '{model}'");
            let brand = preamp_brand(model).expect("brand");
            assert!(!brand.is_empty(), "brand empty for '{model}'");
            let label = preamp_type_label(model).expect("type_label");
            assert!(!label.is_empty(), "type_label empty for '{model}'");
        }
    }

    #[test]
    fn registry_schema_defaults_normalize_for_all_models() {
        for model in supported_models() {
            let schema = preamp_model_schema(model).expect("schema");
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
            let processor = build_preamp_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            )
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
            let processor = build_preamp_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            )
            .unwrap_or_else(|e| panic!("build(Stereo) failed for native '{model}': {e}"));
            assert!(
                matches!(processor, BlockProcessor::Stereo(_)),
                "native '{model}' Stereo build should return Stereo variant"
            );
        }
    }

    #[test]
    fn registry_process_native_mono_silence_produces_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_preamp_processor_for_layout(
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
            let mut proc = match build_preamp_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            )
            .unwrap()
            {
                BlockProcessor::Stereo(p) => p,
                BlockProcessor::Mono(_) => panic!("expected Stereo for '{model}'"),
            };
            for i in 0..256 {
                let [l, r] = proc.process_frame([0.0, 0.0]);
                assert!(
                    l.is_finite() && r.is_finite(),
                    "native stereo '{model}' produced non-finite at frame {i}: [{l}, {r}]"
                );
            }
        }
    }

    #[test]
    fn registry_process_native_mono_signal_produces_non_nan() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_preamp_processor_for_layout(
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

    #[test]
    fn registry_process_native_mono_block_1024_silence_all_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_preamp_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Mono,
            )
            .unwrap()
            {
                BlockProcessor::Mono(p) => p,
                BlockProcessor::Stereo(_) => panic!("expected Mono for '{model}'"),
            };
            let mut buf = vec![0.0_f32; 1024];
            proc.process_block(&mut buf);
            for (i, &s) in buf.iter().enumerate() {
                assert!(
                    s.is_finite(),
                    "native mono '{model}' block silence non-finite at {i}: {s}"
                );
            }
        }
    }

    #[test]
    fn registry_process_native_mono_block_1024_sine_all_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_preamp_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Mono,
            )
            .unwrap()
            {
                BlockProcessor::Mono(p) => p,
                BlockProcessor::Stereo(_) => panic!("expected Mono for '{model}'"),
            };
            let mut buf: Vec<f32> = (0..1024)
                .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
                .collect();
            proc.process_block(&mut buf);
            for (i, &s) in buf.iter().enumerate() {
                assert!(
                    s.is_finite(),
                    "native mono '{model}' block sine non-finite at {i}: {s}"
                );
            }
        }
    }

    #[test]
    fn registry_process_native_stereo_block_1024_sine_all_finite() {
        for model in supported_models().iter().filter(|m| is_native(m)) {
            let params = defaults_for(model);
            let mut proc = match build_preamp_processor_for_layout(
                model,
                &params,
                44100.0,
                AudioChannelLayout::Stereo,
            )
            .unwrap()
            {
                BlockProcessor::Stereo(p) => p,
                BlockProcessor::Mono(_) => panic!("expected Stereo for '{model}'"),
            };
            let mut buf: Vec<[f32; 2]> = (0..1024)
                .map(|i| {
                    let s = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                    [s, s]
                })
                .collect();
            proc.process_block(&mut buf);
            for (i, &[l, r]) in buf.iter().enumerate() {
                assert!(
                    l.is_finite() && r.is_finite(),
                    "native stereo '{model}' block sine non-finite at {i}: [{l}, {r}]"
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
            let _ = build_preamp_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );
        }
    }

    // ── existing specific test (kept) ───────────────────────────────────

    #[test]
    fn supported_preamp_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = preamp_model_schema(model).expect("schema should exist");
            assert_eq!(schema.model, *model);
            assert_eq!(schema.effect_type, "preamp");
            assert!(!schema.parameters.is_empty(), "model '{model}' should expose parameters");
        }
    }
