mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BodyBackendKind {
    Ir,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn body_backend_kind(model: &str) -> Result<BodyBackendKind> {
    Ok(registry::find_model_definition(model)?.backend_kind)
}

pub fn body_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            BodyBackendKind::Ir => "IR",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn body_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn body_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_body_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_body_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_body_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_body_processor_for_layout(
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
    use block_core::AudioChannelLayout;
    use crate::{build_body_processor_for_layout, body_model_schema, supported_models};

    #[test]
    fn supported_bodies_expose_valid_schema() {
        for model in supported_models() {
            let schema = body_model_schema(model).expect("body schema should exist");
            assert_eq!(schema.model, *model);
            assert!(!schema.parameters.is_empty(), "model '{model}' should expose parameters");
        }
    }

    #[test]
    fn supported_bodies_build_for_mono_chains() {
        for model in supported_models() {
            let schema = body_model_schema(model).expect("schema should exist");
            let params = ParameterSet::default()
                .normalized_against(&schema)
                .expect("defaults should normalize");

            let processor = build_body_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );

            assert!(processor.is_ok(), "expected '{model}' to build for mono chains");
        }
    }
}
