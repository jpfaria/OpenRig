//! Gain blocks such as boost, overdrive, distortion, and fuzz.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
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
    _sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, layout)
}

#[cfg(test)]
mod tests {
    use super::{gain_model_schema, supported_models};

    #[test]
    fn supported_gain_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = gain_model_schema(model).expect("schema should exist");
            assert_eq!(schema.model, *model);
            assert_eq!(schema.effect_type, "gain");
            assert!(
                !schema.parameters.is_empty(),
                "model '{model}' should expose parameters"
            );
        }
    }
}
