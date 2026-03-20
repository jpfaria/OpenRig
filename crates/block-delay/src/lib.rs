//! Delay implementations.
mod registry;
pub mod shared;
use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn delay_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_delay_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_delay_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_delay_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{build_delay_processor_for_layout, delay_model_schema, supported_models};
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;

    #[test]
    fn supported_delay_models_expose_schema() {
        for model in supported_models() {
            assert!(
                delay_model_schema(model).is_ok(),
                "expected '{model}' to be supported"
            );
        }
    }

    #[test]
    fn supported_delay_models_build_for_stereo_chains() {
        for model in supported_models() {
            let schema = delay_model_schema(model).expect("schema");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("normalized defaults");
            let processor = build_delay_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Stereo,
            );

            assert!(processor.is_ok(), "{model} should accept stereo chains");
        }
    }
}
