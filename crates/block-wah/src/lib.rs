mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum WahBackendKind {
    Native,
    Nam,
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn wah_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            WahBackendKind::Native => "NATIVE",
            WahBackendKind::Nam => "NAM",
            WahBackendKind::Ir => "IR",
        },
    })
}

pub fn wah_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn validate_wah_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_wah_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_wah_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_wah_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, ModelAudioMode};

    use crate::{build_wah_processor_for_layout, supported_models, validate_wah_params, wah_model_schema};

    #[test]
    fn cry_classic_schema_is_public() {
        let schema = wah_model_schema("cry_classic").expect("schema should exist");
        assert_eq!(schema.effect_type, "wah");
        assert_eq!(schema.model, "cry_classic");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
    }

    #[test]
    fn supported_wah_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = wah_model_schema(model).expect("schema should exist");
            assert_eq!(schema.effect_type, "wah");
            assert_eq!(schema.model, *model);
        }
    }

    #[test]
    fn cry_classic_defaults_normalize_and_build() {
        let schema = wah_model_schema("cry_classic").expect("schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        validate_wah_params("cry_classic", &params).expect("params should validate");
        let processor = build_wah_processor_for_layout(
            "cry_classic",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );
        assert!(processor.is_ok());
    }
}
