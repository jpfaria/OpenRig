use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::FullRigBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct FullRigModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: FullRigBackendKind,
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, AudioChannelLayout) -> Result<BlockProcessor>,
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static FullRigModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported full-rig model '{}'", model))
}
