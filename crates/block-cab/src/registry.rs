use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::CabBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct CabModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: CabBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
}
include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static CabModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported cab model '{}'", model))
}
