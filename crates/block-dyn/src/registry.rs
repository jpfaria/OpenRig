use anyhow::{anyhow, bail, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

use crate::{compressor_studio_clean, gate_basic};

pub struct DynModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn compressor_schema() -> Result<ModelParameterSchema> {
    Ok(compressor_studio_clean::model_schema())
}

fn compressor_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(
            compressor_studio_clean::build_processor(params, sample_rate)?,
        )),
        AudioChannelLayout::Stereo => bail!(
            "compressor model '{}' is mono-only and cannot build native stereo processing",
            compressor_studio_clean::MODEL_ID
        ),
    }
}

fn gate_schema() -> Result<ModelParameterSchema> {
    Ok(gate_basic::model_schema())
}

fn gate_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(gate_basic::build_processor(params, sample_rate)?))
        }
        AudioChannelLayout::Stereo => bail!(
            "gate model '{}' is mono-only and cannot build native stereo processing",
            gate_basic::MODEL_ID
        ),
    }
}

const COMPRESSOR_STUDIO_CLEAN: DynModelDefinition = DynModelDefinition {
    id: compressor_studio_clean::MODEL_ID,
    schema: compressor_schema,
    build: compressor_build,
};

const GATE_BASIC: DynModelDefinition = DynModelDefinition {
    id: gate_basic::MODEL_ID,
    schema: gate_schema,
    build: gate_build,
};

pub const COMPRESSOR_SUPPORTED_MODELS: &[&str] = &[COMPRESSOR_STUDIO_CLEAN.id];
pub const GATE_SUPPORTED_MODELS: &[&str] = &[GATE_BASIC.id];

const COMPRESSOR_MODEL_DEFINITIONS: &[DynModelDefinition] = &[COMPRESSOR_STUDIO_CLEAN];
const GATE_MODEL_DEFINITIONS: &[DynModelDefinition] = &[GATE_BASIC];

pub fn find_compressor_model_definition(model: &str) -> Result<&'static DynModelDefinition> {
    COMPRESSOR_MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported compressor model '{}'", model))
}

pub fn find_gate_model_definition(model: &str) -> Result<&'static DynModelDefinition> {
    GATE_MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported gate model '{}'", model))
}
