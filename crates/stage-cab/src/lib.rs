pub mod marshall_4x12_v30;

use anyhow::{bail, Result};
use marshall_4x12_v30::{
    asset_summary as marshall_4x12_v30_asset_summary,
    build_processor_for_model as build_marshall_4x12_v30_processor,
    model_schema as marshall_4x12_v30_model_schema,
    supports_model as supports_marshall_4x12_v30_model,
    validate_params as validate_marshall_4x12_v30_params,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, StageProcessor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CabBackendKind {
    Ir,
}

pub fn cab_backend_kind(model: &str) -> Result<CabBackendKind> {
    if supports_marshall_4x12_v30_model(model) {
        Ok(CabBackendKind::Ir)
    } else {
        bail!("unsupported cab model '{}'", model)
    }
}

pub fn cab_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_marshall_4x12_v30_model(model) {
        Ok(marshall_4x12_v30_model_schema())
    } else {
        bail!("unsupported cab model '{}'", model)
    }
}

pub fn cab_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    if supports_marshall_4x12_v30_model(model) {
        marshall_4x12_v30_asset_summary(params)
    } else {
        bail!("unsupported cab model '{}'", model)
    }
}

pub fn validate_cab_params(model: &str, params: &ParameterSet) -> Result<()> {
    if supports_marshall_4x12_v30_model(model) {
        validate_marshall_4x12_v30_params(params)
    } else {
        bail!("unsupported cab model '{}'", model)
    }
}

pub fn build_cab_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_cab_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_cab_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_marshall_4x12_v30_model(model) {
        build_marshall_4x12_v30_processor(params, sample_rate, layout)
    } else {
        bail!("unsupported cab model '{}'", model)
    }
}

#[cfg(test)]
mod tests {
    use domain::value_objects::ParameterValue;
    use stage_core::param::ParameterSet;
    use stage_core::{AudioChannelLayout, ModelAudioMode, StageProcessor};

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
            StageProcessor::Mono(_) => {}
            StageProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = cab_asset_summary("marshall_4x12_v30", &params)
            .expect("asset summary should resolve");
        assert!(summary.contains("ev_mix_b.wav"));
    }
}
