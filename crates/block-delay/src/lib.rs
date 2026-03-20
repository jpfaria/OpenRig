//! Delay implementations.
pub mod analog_warm;
pub mod digital_clean;
pub mod modulated_delay;
mod registry;
pub mod reverse;
pub mod slapback;
pub mod tape_vintage;

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
    use super::{build_delay_processor_for_layout, delay_model_schema};
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;

    #[test]
    fn new_delay_catalog_is_publicly_supported() {
        for model in [
            "digital_clean",
            "analog_warm",
            "tape_vintage",
            "reverse",
            "slapback",
            "modulated_delay",
        ] {
            assert!(
                delay_model_schema(model).is_ok(),
                "expected '{model}' to be supported"
            );
        }
    }

    #[test]
    fn legacy_delay_models_are_rejected() {
        for model in ["digital_basic", "digital_ping_pong", "digital_wide"] {
            assert!(
                delay_model_schema(model).is_err(),
                "expected legacy model '{model}' to be rejected"
            );
        }
    }

    #[test]
    fn digital_clean_builds_for_stereo_chains() {
        let schema = delay_model_schema("digital_clean").expect("schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("normalized defaults");
        let processor = build_delay_processor_for_layout(
            "digital_clean",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );

        assert!(
            processor.is_ok(),
            "digital_clean should accept stereo chains"
        );
    }
}
