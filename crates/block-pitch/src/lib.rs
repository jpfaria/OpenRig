//! Pitch correction block implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PitchBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn pitch_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            PitchBackendKind::Native => "NATIVE",
            PitchBackendKind::Nam => "NAM",
            PitchBackendKind::Ir => "IR",
            PitchBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn pitch_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn validate_pitch_params(model: &str, params: &ParameterSet) -> Result<()> {
    let schema = pitch_model_schema(model)?;
    params
        .normalized_against(&schema)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!(error))
}

pub fn build_pitch_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_pitch_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Stereo)
}

pub fn build_pitch_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{pitch_model_schema, supported_models, validate_pitch_params};
    use block_core::param::ParameterSet;

    #[test]
    fn exposes_x42_autotune() {
        let models = supported_models();
        assert!(
            models.contains(&"lv2_fat1_autotune"),
            "should contain x42 autotune"
        );
    }

    #[test]
    fn x42_schema_is_pitch() {
        let schema = pitch_model_schema("lv2_fat1_autotune").expect("schema");
        assert_eq!(schema.effect_type, "pitch");
        assert_eq!(schema.model, "lv2_fat1_autotune");
    }

    #[test]
    fn defaults_normalize_x42() {
        let params = ParameterSet::default();
        validate_pitch_params("lv2_fat1_autotune", &params)
            .expect("defaults should normalize");
    }
}
