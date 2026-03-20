pub mod bogner_ecstasy;
pub mod native;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn amp_combo_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn amp_combo_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_amp_combo_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_amp_combo_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_amp_combo_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_amp_combo_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{amp_combo_model_schema, build_amp_combo_processor_for_layout};
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, ModelAudioMode};

    #[test]
    fn native_amp_combo_catalog_is_publicly_supported() {
        for model in [
            "blackface_clean_combo",
            "tweed_breakup_combo",
            "chime_combo",
        ] {
            let schema = amp_combo_model_schema(model).expect("schema should exist");
            assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
            assert_eq!(schema.parameters.len(), 10);
        }
    }

    #[test]
    fn native_amp_combos_build_for_stereo_chains() {
        for model in [
            "blackface_clean_combo",
            "tweed_breakup_combo",
            "chime_combo",
        ] {
            let schema = amp_combo_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_amp_combo_processor_for_layout(
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
