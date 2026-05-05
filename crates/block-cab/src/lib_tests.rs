
use crate::{
    build_cab_processor_for_layout, cab_backend_kind, cab_model_schema, supported_models,
    CabBackendKind,
};
use block_core::param::ParameterSet;
use block_core::AudioChannelLayout;

#[test]
#[ignore]
fn supported_cabs_expose_valid_schema() {
    for model in supported_models() {
        let schema = cab_model_schema(model).expect("cab schema should exist");
        assert_eq!(schema.model, *model);
        assert!(
            !schema.parameters.is_empty(),
            "model '{model}' should expose parameters"
        );
    }
}

#[test]
#[ignore]
fn supported_cabs_build_for_mono_chains() {
    for model in supported_models() {
        let schema = cab_model_schema(model).expect("schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");

        let processor =
            build_cab_processor_for_layout(model, &params, 48_000.0, AudioChannelLayout::Mono);

        assert!(
            processor.is_ok(),
            "expected '{model}' to build for mono chains"
        );
    }
}

#[test]
fn native_cabs_build_for_stereo_chains() {
    for model in supported_models() {
        if !matches!(
            cab_backend_kind(model).expect("backend"),
            CabBackendKind::Native
        ) {
            continue;
        }
        let schema = cab_model_schema(model).expect("schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");

        let processor =
            build_cab_processor_for_layout(model, &params, 48_000.0, AudioChannelLayout::Stereo);

        assert!(
            processor.is_ok(),
            "expected '{model}' to build for stereo chains"
        );
    }
}
