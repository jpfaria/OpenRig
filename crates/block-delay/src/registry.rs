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
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
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

/// Returns true if the model has a usable backend on the current platform.
/// LV2 wrappers report `false` when their plugin binary is missing from
/// `libs/lv2/<platform>/`. Native/NAM/IR/VST3 models report `true` (they are
/// always considered available; per-asset checks happen at instantiation).
pub fn is_model_available(model: &str) -> bool {
    AVAILABLE_MODEL_IDS.iter().any(|id| *id == model)
        || !MODEL_DEFINITIONS.iter().any(|d| d.id == model)
}
