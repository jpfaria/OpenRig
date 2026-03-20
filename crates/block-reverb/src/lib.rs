//! Reverb implementations.
pub mod plate_foundation;
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, NamedModel, BlockProcessor};

pub enum ReverbModel {
    PlateFoundation,
}

impl NamedModel for ReverbModel {
    fn model_key(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => registry::SUPPORTED_MODELS[0],
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            ReverbModel::PlateFoundation => registry::find_model_definition(self.model_key())
                .map(|definition| definition.display_name)
                .unwrap_or("Plate Foundation Reverb"),
        }
    }
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn reverb_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn build_reverb_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_reverb_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_reverb_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}
