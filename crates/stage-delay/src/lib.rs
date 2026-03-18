//! Delay implementations.
pub mod digital_basic;
pub mod digital_ping_pong;
pub mod digital_wide;

use anyhow::{bail, Result};
use digital_basic::{
    build_mono_processor, model_schema as basic_model_schema,
    supports_model as supports_basic_model,
};
use digital_ping_pong::{
    build_stereo_processor as build_ping_pong_processor, model_schema as ping_pong_model_schema,
    supports_model as supports_ping_pong_model,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, NamedModel, StageProcessor};
use digital_wide::{
    build_stereo_processor as build_wide_processor, model_schema as wide_model_schema,
    supports_model as supports_wide_model,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayModel {
    DigitalBasic,
    DigitalPingPong,
    DigitalWide,
}

impl NamedModel for DelayModel {
    fn model_key(&self) -> &'static str {
        match self {
            DelayModel::DigitalBasic => digital_basic::MODEL_ID,
            DelayModel::DigitalPingPong => digital_ping_pong::MODEL_ID,
            DelayModel::DigitalWide => digital_wide::MODEL_ID,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            DelayModel::DigitalBasic => "Digital Basic Delay",
            DelayModel::DigitalPingPong => "Digital Ping Pong Delay",
            DelayModel::DigitalWide => "Digital Wide Delay",
        }
    }
}

pub fn delay_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_basic_model(model) {
        Ok(basic_model_schema())
    } else if supports_ping_pong_model(model) {
        Ok(ping_pong_model_schema())
    } else if supports_wide_model(model) {
        Ok(wide_model_schema())
    } else {
        bail!("unsupported delay model '{}'", model)
    }
}

pub fn build_delay_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_delay_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_delay_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_basic_model(model) {
        match layout {
            AudioChannelLayout::Mono => Ok(StageProcessor::Mono(build_mono_processor(
                params,
                sample_rate,
            )?)),
            AudioChannelLayout::Stereo => bail!(
                "delay model '{}' is mono-only and cannot build native stereo processing",
                model
            ),
        }
    } else if supports_ping_pong_model(model) {
        match layout {
            AudioChannelLayout::Stereo => Ok(StageProcessor::Stereo(build_ping_pong_processor(
                params,
                sample_rate,
            )?)),
            AudioChannelLayout::Mono => {
                bail!("delay model '{}' requires a stereo track", model)
            }
        }
    } else if supports_wide_model(model) {
        match layout {
            AudioChannelLayout::Stereo => Ok(StageProcessor::Stereo(build_wide_processor(
                params,
                sample_rate,
            )?)),
            AudioChannelLayout::Mono => {
                bail!("delay model '{}' requires a stereo track", model)
            }
        }
    } else {
        bail!("unsupported delay model '{}'", model)
    }
}
