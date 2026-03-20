use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::roland_jc_120b_jazz_chorus;

pub struct FullRigModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn roland_schema() -> Result<ModelParameterSchema> {
    Ok(roland_jc_120b_jazz_chorus::model_schema())
}

fn roland_build(params: &ParameterSet, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    roland_jc_120b_jazz_chorus::build_processor_for_model(params, layout)
}

const ROLAND_JC_120B_JAZZ_CHORUS: FullRigModelDefinition = FullRigModelDefinition {
    id: roland_jc_120b_jazz_chorus::MODEL_ID,
    schema: roland_schema,
    validate: roland_jc_120b_jazz_chorus::validate_params,
    asset_summary: roland_jc_120b_jazz_chorus::asset_summary,
    build: roland_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[ROLAND_JC_120B_JAZZ_CHORUS.id];

const MODEL_DEFINITIONS: &[FullRigModelDefinition] = &[ROLAND_JC_120B_JAZZ_CHORUS];

pub fn find_model_definition(model: &str) -> Result<&'static FullRigModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported full-rig model '{}'", model))
}
