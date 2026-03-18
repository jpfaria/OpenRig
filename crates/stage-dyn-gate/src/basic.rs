use anyhow::{Error, Result};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{db_to_lin, EnvelopeFollower, ModelChannelSupport, MonoProcessor};

pub const MODEL_ID: &str = "gate_basic";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateParams {
    pub threshold: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            threshold: -60.0,
            attack_ms: 5.0,
            release_ms: 50.0,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "noise_gate_basic" | "gate" | "basic")
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "gate".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Noise Gate".to_string(),
        channel_support: ModelChannelSupport::Mono,
        stereo_processing: None,
        parameters: vec![
            float_parameter(
                "threshold",
                "Threshold",
                None,
                Some(GateParams::default().threshold),
                -96.0,
                0.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "attack_ms",
                "Attack",
                None,
                Some(GateParams::default().attack_ms),
                0.1,
                100.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "release_ms",
                "Release",
                None,
                Some(GateParams::default().release_ms),
                1.0,
                500.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<GateParams> {
    Ok(GateParams {
        threshold: required_f32(params, "threshold").map_err(Error::msg)?,
        attack_ms: required_f32(params, "attack_ms").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
    })
}

pub struct BasicNoiseGate {
    threshold: f32,
    envelope: EnvelopeFollower,
}

impl BasicNoiseGate {
    pub fn new(threshold_db: f32, attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        Self {
            threshold: db_to_lin(threshold_db),
            envelope: EnvelopeFollower::from_ms(attack_ms, release_ms, sample_rate),
        }
    }
}

impl MonoProcessor for BasicNoiseGate {
    fn process_sample(&mut self, input: f32) -> f32 {
        let env = self.envelope.process(input.abs());
        if env >= self.threshold {
            input
        } else {
            0.0
        }
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(BasicNoiseGate::new(
        params.threshold,
        params.attack_ms,
        params.release_ms,
        sample_rate,
    )))
}
