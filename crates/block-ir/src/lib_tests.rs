    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, ModelAudioMode};
    use domain::value_objects::ParameterValue;

    use crate::{build_ir_processor_for_layout, ir_model_schema, supported_models, validate_ir_params};

    #[test]
    fn generic_ir_schema_is_public() {
        let schema = ir_model_schema("generic_ir").expect("schema should exist");
        assert_eq!(schema.effect_type, "ir");
        assert_eq!(schema.model, "generic_ir");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        assert!(schema.parameters.iter().any(|p| p.path == "file"));
    }

    #[test]
    fn supported_ir_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = ir_model_schema(model).expect("schema should exist");
            assert_eq!(schema.effect_type, "ir");
            assert_eq!(schema.model, *model);
        }
    }

    #[test]
    fn generic_ir_rejects_missing_file() {
        let params = ParameterSet::default();
        let error = validate_ir_params("generic_ir", &params).expect_err("validation should fail");
        assert!(error.to_string().contains("file"));
    }

    #[test]
    fn generic_ir_build_requires_existing_file() {
        let mut params = ParameterSet::default();
        params.insert("file", ParameterValue::String("/tmp/does-not-exist.wav".into()));
        let result =
            build_ir_processor_for_layout("generic_ir", &params, 48_000.0, AudioChannelLayout::Mono);
        assert!(result.is_err());
    }
