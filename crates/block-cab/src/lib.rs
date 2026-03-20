pub mod marshall_4x12_v30;
pub mod native;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CabBackendKind {
    Ir,
    Native,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn cab_backend_kind(model: &str) -> Result<CabBackendKind> {
    Ok(registry::find_model_definition(model)?.backend_kind)
}

pub fn cab_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn cab_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_cab_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_cab_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_cab_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_cab_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use domain::value_objects::ParameterValue;
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

    use crate::{
        build_cab_processor_for_layout, cab_asset_summary, cab_model_schema, validate_cab_params,
    };

    #[test]
    fn marshall_4x12_v30_schema_exposes_capture_select() {
        let schema = cab_model_schema("marshall_4x12_v30").expect("cab schema should exist");

        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        assert_eq!(schema.parameters.len(), 1);
        assert_eq!(schema.parameters[0].path, "capture");
    }

    #[test]
    fn marshall_4x12_v30_rejects_unknown_capture() {
        let mut params = ParameterSet::default();
        params.insert("capture", ParameterValue::String("unknown".into()));

        let error = validate_cab_params("marshall_4x12_v30", &params)
            .expect_err("unknown capture should fail");

        assert!(error.to_string().contains("capture"));
    }

    #[test]
    fn marshall_4x12_v30_builds_mono_processor_for_curated_capture() {
        let mut params = ParameterSet::default();
        params.insert("capture", ParameterValue::String("ev_mix_b".into()));

        let processor = build_cab_processor_for_layout(
            "marshall_4x12_v30",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("cab processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary =
            cab_asset_summary("marshall_4x12_v30", &params).expect("asset summary should resolve");
        assert!(summary.contains("cab.marshall_4x12_v30.ev_mix_b"));
    }

    #[test]
    fn native_cab_catalog_is_publicly_supported() {
        for model in ["brit_4x12_cab", "american_2x12_cab", "vintage_1x12_cab"] {
            let schema = cab_model_schema(model).expect("cab schema should exist");
            assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
            assert_eq!(schema.parameters.len(), 8);
        }
    }

    #[test]
    fn native_cabs_build_for_stereo_chains() {
        for model in ["brit_4x12_cab", "american_2x12_cab", "vintage_1x12_cab"] {
            let schema = cab_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_cab_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            );

            assert!(
                processor.is_ok(),
                "expected '{model}' to build for stereo chains"
            );
        }
    }
}
