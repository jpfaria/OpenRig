use anyhow::{anyhow, Result};
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor};

use crate::{
    analog_warm, digital_clean, modulated_delay, reverse, shared::build_dual_mono_from_builder,
    slapback, tape_vintage,
};

pub struct DelayModelDefinition {
    pub id: &'static str,
    pub schema: fn() -> Result<ModelParameterSchema>,
    pub build: fn(&ParameterSet, f32, AudioChannelLayout) -> Result<BlockProcessor>,
}

fn build_dual_mono_delay_processor<F>(
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

fn digital_clean_schema() -> Result<ModelParameterSchema> {
    Ok(digital_clean::model_schema())
}

fn digital_clean_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_dual_mono_delay_processor(layout, || {
        digital_clean::build_mono_processor(params, sample_rate)
    })
}

fn analog_warm_schema() -> Result<ModelParameterSchema> {
    Ok(analog_warm::model_schema())
}

fn analog_warm_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_dual_mono_delay_processor(layout, || {
        analog_warm::build_mono_processor(params, sample_rate)
    })
}

fn tape_vintage_schema() -> Result<ModelParameterSchema> {
    Ok(tape_vintage::model_schema())
}

fn tape_vintage_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_dual_mono_delay_processor(layout, || {
        tape_vintage::build_mono_processor(params, sample_rate)
    })
}

fn reverse_schema() -> Result<ModelParameterSchema> {
    Ok(reverse::model_schema())
}

fn reverse_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_dual_mono_delay_processor(layout, || reverse::build_mono_processor(params, sample_rate))
}

fn slapback_schema() -> Result<ModelParameterSchema> {
    Ok(slapback::model_schema())
}

fn slapback_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_dual_mono_delay_processor(layout, || slapback::build_mono_processor(params, sample_rate))
}

fn modulated_delay_schema() -> Result<ModelParameterSchema> {
    Ok(modulated_delay::model_schema())
}

fn modulated_delay_build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_dual_mono_delay_processor(layout, || {
        modulated_delay::build_mono_processor(params, sample_rate)
    })
}

const DIGITAL_CLEAN: DelayModelDefinition = DelayModelDefinition {
    id: digital_clean::MODEL_ID,
    schema: digital_clean_schema,
    build: digital_clean_build,
};

const ANALOG_WARM: DelayModelDefinition = DelayModelDefinition {
    id: analog_warm::MODEL_ID,
    schema: analog_warm_schema,
    build: analog_warm_build,
};

const TAPE_VINTAGE: DelayModelDefinition = DelayModelDefinition {
    id: tape_vintage::MODEL_ID,
    schema: tape_vintage_schema,
    build: tape_vintage_build,
};

const REVERSE: DelayModelDefinition = DelayModelDefinition {
    id: reverse::MODEL_ID,
    schema: reverse_schema,
    build: reverse_build,
};

const SLAPBACK: DelayModelDefinition = DelayModelDefinition {
    id: slapback::MODEL_ID,
    schema: slapback_schema,
    build: slapback_build,
};

const MODULATED_DELAY: DelayModelDefinition = DelayModelDefinition {
    id: modulated_delay::MODEL_ID,
    schema: modulated_delay_schema,
    build: modulated_delay_build,
};

pub const SUPPORTED_MODELS: &[&str] = &[
    DIGITAL_CLEAN.id,
    ANALOG_WARM.id,
    TAPE_VINTAGE.id,
    REVERSE.id,
    SLAPBACK.id,
    MODULATED_DELAY.id,
];

const MODEL_DEFINITIONS: &[DelayModelDefinition] = &[
    DIGITAL_CLEAN,
    ANALOG_WARM,
    TAPE_VINTAGE,
    REVERSE,
    SLAPBACK,
    MODULATED_DELAY,
];

pub fn find_model_definition(model: &str) -> Result<&'static DelayModelDefinition> {
    MODEL_DEFINITIONS
        .iter()
        .find(|definition| definition.id == model)
        .ok_or_else(|| anyhow!("unsupported delay model '{}'", model))
}
