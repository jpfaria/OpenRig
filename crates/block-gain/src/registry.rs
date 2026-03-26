use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::GainBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct GainModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: GainBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static GainModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported gain model '{}'", model))
}
