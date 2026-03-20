use anyhow::{anyhow, bail, Result};
use block_core::param::ModelParameterSchema;
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::eq_three_band_basic;

pub struct FilterModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn eq_schema() -> Result<ModelParameterSchema> {
    Ok(eq_three_band_basic::model_schema())
}

fn eq_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(eq_three_band_basic::build_processor(
                params,
                sample_rate,
            )?))
        }
        AudioChannelLayout::Stereo => bail!(
            "eq model '{}' is mono-only and cannot build native stereo processing",
            eq_three_band_basic::MODEL_ID
        ),
    }
}

const EQ_THREE_BAND_BASIC: FilterModelDefinition = FilterModelDefinition {
    id: eq_three_band_basic::MODEL_ID,
    schema: eq_schema,
    build: eq_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[EQ_THREE_BAND_BASIC.id];

const MODEL_DEFINITIONS: &[FilterModelDefinition] = &[EQ_THREE_BAND_BASIC];

pub fn find_model_definition(model: &str) -> Result<&'static FilterModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported eq model '{}'", model))
}
