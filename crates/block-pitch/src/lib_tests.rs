    use super::{
        build_pitch_processor_for_layout, pitch_model_schema, supported_models,
        validate_pitch_params, pitch_display_name, pitch_brand, pitch_type_label,
    };
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;

    // ── helpers ──────────────────────────────────────────────────────────

    fn defaults_for(model: &str) -> ParameterSet {
        let schema = pitch_model_schema(model).expect("schema");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    // ── registry-wide tests ─────────────────────────────────────────────

    #[test]
    fn registry_schema_all_models_return_non_empty_schema() {
        for model in supported_models() {
            let schema = pitch_model_schema(model)
                .unwrap_or_else(|e| panic!("schema() failed for '{model}': {e}"));
            assert_eq!(schema.model, *model, "schema.model mismatch for '{model}'");
            assert_eq!(schema.effect_type, "pitch", "effect_type mismatch for '{model}'");
        }
    }

    #[test]
    fn registry_validate_all_models_accept_empty_params() {
        for model in supported_models() {
            // Use empty ParameterSet because validate_pitch_params internally
            // calls normalized_against which fills defaults without re-validating them.
            validate_pitch_params(model, &ParameterSet::default())
                .unwrap_or_else(|e| panic!("validate() rejected empty params for '{model}': {e}"));
        }
    }

    #[test]
    fn registry_metadata_all_models_have_display_name_and_brand() {
        for model in supported_models() {
            let name = pitch_display_name(model);
            assert!(!name.is_empty(), "display_name empty for '{model}'");
            let brand = pitch_brand(model);
            assert!(!brand.is_empty(), "brand empty for '{model}'");
            let label = pitch_type_label(model);
            assert!(!label.is_empty(), "type_label empty for '{model}'");
        }
    }

    #[test]
    fn registry_schema_defaults_normalize_for_all_models() {
        for model in supported_models() {
            let schema = pitch_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(
                result.is_ok(),
                "defaults failed to normalize for '{model}': {}",
                result.unwrap_err()
            );
        }
    }

    // ── LV2 models: build requires external plugins, skip ───────────

    #[test]
    #[ignore]
    fn registry_build_lv2_models_ignored() {
        for model in supported_models() {
            let params = defaults_for(model);
            let _ = build_pitch_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );
        }
    }

