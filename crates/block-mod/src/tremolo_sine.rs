use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "tremolo_sine";

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

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "sine_tremolo" | "tremolo" | "basic")
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "tremolo".to_string(),
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
