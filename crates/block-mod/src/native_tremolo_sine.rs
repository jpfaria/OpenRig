use anyhow::{Error, Result};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "tremolo_sine";
pub const DISPLAY_NAME: &str = "Sine Tremolo";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TremoloParams {
    pub rate_hz: f32,
    pub depth: f32,
}

impl Default for TremoloParams {
    fn default() -> Self {
        Self {
            rate_hz: 4.0,
            depth: 0.5,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Sine Tremolo".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(TremoloParams::default().rate_hz),
                0.1,
                20.0,
                0.1,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(TremoloParams::default().depth),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<TremoloParams> {
    Ok(TremoloParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)?,
    })
}

pub struct SineTremolo {
    rate_hz: f32,
    depth: f32,
    sample_rate: f32,
    phase: f32,
}

impl SineTremolo {
    pub fn new(rate_hz: f32, depth: f32, sample_rate: f32) -> Self {
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
        }
    }
}

impl MonoProcessor for SineTremolo {
    fn process_sample(&mut self, input: f32) -> f32 {
        let lfo = 0.5 * (1.0 + self.phase.sin());
        let gain = 1.0 - (self.depth * lfo);
        self.phase = (self.phase + (TAU * self.rate_hz / self.sample_rate)).rem_euclid(TAU);
        input * gain
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(SineTremolo::new(
        params.rate_hz,
        params.depth,
        sample_rate,
    )))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    match layout {
        block_core::AudioChannelLayout::Mono => {
            Ok(block_core::BlockProcessor::Mono(build_processor(params, sample_rate)?))
        }
        block_core::AudioChannelLayout::Stereo => {
            struct DualMonoProcessor {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for DualMonoProcessor {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(DualMonoProcessor {
                left: build_processor(params, sample_rate)?,
                right: build_processor(params, sample_rate)?,
            })))
        }
    }
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: ModBackendKind::Native,
    panel_bg: [0x2c, 0x2e, 0x34],
    panel_text: [0x80, 0x90, 0xa0],
    brand_strip_bg: [0x1a, 0x1a, 0x1a],
    model_font: "",
    schema,
    build,
};
