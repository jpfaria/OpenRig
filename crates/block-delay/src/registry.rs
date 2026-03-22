use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor};

use crate::shared::build_dual_mono_from_builder;
use crate::DelayBackendKind;

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct DelayModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub brand: &'static str,
    pub backend_kind: DelayBackendKind,
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

pub(crate) fn build_dual_mono_delay_processor<F>(
    layout: AudioChannelLayout,
    builder: F,
) -> Result<BlockProcessor>
where
    F: Fn() -> Result<Box<dyn MonoProcessor>>,
{
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(builder()?)),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(build_dual_mono_from_builder(
            builder,
        )?)),
    }
}

include!(concat!(env!("OUT_DIR"), "/generated_registry.rs"));

pub fn find_model_definition(model: &str) -> Result<&'static DelayModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported delay model '{}'", model))
}
