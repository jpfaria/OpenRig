use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::AmpHeadBackendKind;

#[derive(Clone, Copy)]
pub struct AmpHeadModelDefinition {
    pub id: &'static str,
    /// Nome de exibição do modelo (ex: "American Clean", "Marshall JCM 800 2203")
    pub display_name: &'static str,
    /// Marca do equipamento (ex: "marshall", "vox", "native")
    pub brand: &'static str,
    pub backend_kind: AmpHeadBackendKind,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub validate: fn(&ParameterSet) -> Result<()>,
    pub asset_summary: fn(&ParameterSet) -> Result<String>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}
include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static AmpHeadModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported amp-head model '{}'", model))
}
