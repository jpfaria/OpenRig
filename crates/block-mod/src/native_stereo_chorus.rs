use anyhow::{Error, Result};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, StereoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "stereo_chorus";
pub const DISPLAY_NAME: &str = "Stereo Chorus";

const BASE_DELAY_MS: f32 = 20.0;
const DEPTH_MS_MAX: f32 = 10.0;
const BUFFER_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub struct ChorusParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub mix: f32,
    pub spread: f32,
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.5,
            depth: 50.0,
            mix: 50.0,
            spread: 100.0,
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
                Some(ChorusParams::default().rate_hz),
                0.1,
                5.0,
                0.1,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(ChorusParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(ChorusParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "spread",
                "Spread",
                None,
                Some(ChorusParams::default().spread),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<ChorusParams> {
    Ok(ChorusParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        spread: required_f32(params, "spread").map_err(Error::msg)? / 100.0,
    })
}

struct DelayLine {
    buffer: [f32; BUFFER_SIZE],
    write_pos: usize,
}

impl DelayLine {
    fn new() -> Self {
        Self {
            buffer: [0.0; BUFFER_SIZE],
            write_pos: 0,
        }
    }

    fn process(&mut self, input: f32, delay_samples: f32) -> f32 {
        self.buffer[self.write_pos] = input;

        let delay_int = delay_samples as usize;
        let frac = delay_samples - delay_int as f32;

        let idx0 = (self.write_pos + BUFFER_SIZE - delay_int) % BUFFER_SIZE;
        let idx1 = (self.write_pos + BUFFER_SIZE - delay_int - 1) % BUFFER_SIZE;
        let out = self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac;

        self.write_pos = (self.write_pos + 1) % BUFFER_SIZE;
        out
    }
}

pub struct StereoChorus {
    left: DelayLine,
    right: DelayLine,
    depth: f32,
    mix: f32,
    phase_l: f32,
    phase_r: f32,
    phase_inc: f32,
    sample_rate: f32,
}

impl StereoChorus {
    pub fn new(params: ChorusParams, sample_rate: f32) -> Self {
        let phase_r = std::f32::consts::PI * params.spread;
        Self {
            left: DelayLine::new(),
            right: DelayLine::new(),
            depth: params.depth,
            mix: params.mix,
            phase_l: 0.0,
            phase_r,
            phase_inc: TAU * params.rate_hz / sample_rate,
            sample_rate,
        }
    }
}

impl StereoProcessor for StereoChorus {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let delay_l = (BASE_DELAY_MS + DEPTH_MS_MAX * self.depth * self.phase_l.sin())
            * self.sample_rate
            / 1000.0;
        let delay_r = (BASE_DELAY_MS + DEPTH_MS_MAX * self.depth * self.phase_r.sin())
            * self.sample_rate
            / 1000.0;

        let wet_l = self.left.process(input[0], delay_l);
        let wet_r = self.right.process(input[1], delay_r);

        self.phase_l = (self.phase_l + self.phase_inc).rem_euclid(TAU);
        self.phase_r = (self.phase_r + self.phase_inc).rem_euclid(TAU);

        [
            input[0] * (1.0 - self.mix) + wet_l * self.mix,
            input[1] * (1.0 - self.mix) + wet_r * self.mix,
        ]
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = params_from_set(params)?;
    Ok(BlockProcessor::Stereo(Box::new(StereoChorus::new(
        p,
        sample_rate,
    ))))
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
