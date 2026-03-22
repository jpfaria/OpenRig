use anyhow::{anyhow, Result};
use block_core::param::ModelParameterSchema;
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::FilterBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct FilterModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: FilterBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static FilterModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported eq model '{}'", model))
}
