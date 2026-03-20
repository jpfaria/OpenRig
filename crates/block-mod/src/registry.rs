use anyhow::{anyhow, bail, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::tremolo_sine;

pub struct ModModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn tremolo_schema() -> Result<ModelParameterSchema> {
    Ok(tremolo_sine::model_schema())
}

fn tremolo_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(tremolo_sine::build_processor(
                params,
                sample_rate,
            )?))
        }
        AudioChannelLayout::Stereo => bail!(
            "tremolo model '{}' is mono-only and cannot build native stereo processing",
            tremolo_sine::MODEL_ID
        ),
    }
}

const TREMOLO_SINE: ModModelDefinition = ModModelDefinition {
    id: tremolo_sine::MODEL_ID,
    schema: tremolo_schema,
    build: tremolo_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[TREMOLO_SINE.id];

const MODEL_DEFINITIONS: &[ModModelDefinition] = &[TREMOLO_SINE];

pub fn find_model_definition(model: &str) -> Result<&'static ModModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported tremolo model '{}'", model))
}
