use anyhow::{anyhow, bail, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::plate_foundation;

pub struct ReverbModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn plate_schema() -> Result<ModelParameterSchema> {
    Ok(plate_foundation::model_schema())
}

fn plate_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(plate_foundation::build_processor(
                params,
                sample_rate,
            )?))
        }
        AudioChannelLayout::Stereo => bail!(
            "reverb model '{}' is mono-only and cannot build native stereo processing",
            plate_foundation::MODEL_ID
        ),
    }
}

const PLATE_FOUNDATION: ReverbModelDefinition = ReverbModelDefinition {
    id: plate_foundation::MODEL_ID,
    display_name: "Plate Foundation Reverb",
    schema: plate_schema,
    build: plate_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[PLATE_FOUNDATION.id];

const MODEL_DEFINITIONS: &[ReverbModelDefinition] = &[PLATE_FOUNDATION];

pub fn find_model_definition(model: &str) -> Result<&'static ReverbModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported reverb model '{}'", model))
}
