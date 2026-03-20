use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::blues_overdrive_bd_2;

pub struct GainModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn blues_schema() -> Result<ModelParameterSchema> {
    Ok(blues_overdrive_bd_2::model_schema())
}

fn blues_build(params: &ParameterSet, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    blues_overdrive_bd_2::build_processor_for_model(params, layout)
}

const BLUES_OVERDRIVE_BD_2: GainModelDefinition = GainModelDefinition {
    id: blues_overdrive_bd_2::MODEL_ID,
    schema: blues_schema,
    validate: blues_overdrive_bd_2::validate_params,
    asset_summary: blues_overdrive_bd_2::asset_summary,
    build: blues_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[BLUES_OVERDRIVE_BD_2.id];

const MODEL_DEFINITIONS: &[GainModelDefinition] = &[BLUES_OVERDRIVE_BD_2];

pub fn find_model_definition(model: &str) -> Result<&'static GainModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported drive model '{}'", model))
}
