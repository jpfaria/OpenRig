use anyhow::{Error, Result};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "vibrato";
pub const DISPLAY_NAME: &str = "Vibrato";

/// Maximum delay line length in milliseconds.
/// The center delay is half this value; depth modulates ±half around center.
const MAX_DELAY_MS: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VibratoParams {
    pub rate_hz: f32,
    pub depth: f32,
}

impl Default for VibratoParams {
    fn default() -> Self {
        Self {
            rate_hz: 4.0,
            depth: 50.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(VibratoParams::default().rate_hz),
                0.1,
                8.0,
                0.1,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(VibratoParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<VibratoParams> {
    Ok(VibratoParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
    })
}

pub struct Vibrato {
    rate_hz: f32,
    depth: f32,
    sample_rate: f32,
    phase: f32,
    buffer: Vec<f32>,
    write_pos: usize,
    center_samples: f32,
}

impl Vibrato {
    pub fn new(rate_hz: f32, depth: f32, sample_rate: f32) -> Self {
        let max_delay_samples = (MAX_DELAY_MS * 0.001 * sample_rate).ceil() as usize;
        let buf_len = max_delay_samples + 2;
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
            buffer: vec![0.0; buf_len],
            write_pos: 0,
            center_samples: max_delay_samples as f32 / 2.0,
        }
    }
}

impl MonoProcessor for Vibrato {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.buffer[self.write_pos] = input;

        // Delay oscillates around center_samples with amplitude = center * depth
        let delay = self.center_samples + self.center_samples * self.depth * self.phase.sin();

        let delay_floor = delay as usize;
        let frac = delay - delay_floor as f32;

        let buf_len = self.buffer.len();
        let read0 = (self.write_pos + buf_len - delay_floor) % buf_len;
        let read1 = (self.write_pos + buf_len - delay_floor - 1) % buf_len;

        let output = self.buffer[read0] * (1.0 - frac) + self.buffer[read1] * frac;

        self.phase = (self.phase + TAU * self.rate_hz / self.sample_rate).rem_euclid(TAU);
        self.write_pos = (self.write_pos + 1) % buf_len;

        output // 100% wet — no dry signal
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(Vibrato::new(params.rate_hz, params.depth, sample_rate)))
}

fn build_processor_with_phase(params: &ParameterSet, sample_rate: f32, phase_offset: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    let mut v = Vibrato::new(params.rate_hz, params.depth, sample_rate);
    v.phase = phase_offset;
    Ok(Box::new(v))
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
            struct StereoVibrato {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoVibrato {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoVibrato {
                left: build_processor(params, sample_rate)?,
                right: build_processor_with_phase(params, sample_rate, std::f32::consts::PI)?,
            })))
        }
    }
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: ModBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
