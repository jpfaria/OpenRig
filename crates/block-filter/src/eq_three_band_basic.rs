use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, ModelAudioMode, MonoProcessor, OnePoleHighPass, OnePoleLowPass};

pub const MODEL_ID: &str = "eq_three_band_basic";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqParams {
    pub low_gain_db: f32,
    pub mid_gain_db: f32,
    pub high_gain_db: f32,
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            low_gain_db: 0.0,
            mid_gain_db: 0.0,
            high_gain_db: 0.0,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "three_band_basic" | "three_band" | "eq")
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "eq".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Three Band EQ".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "low_gain_db",
                "Low",
                None,
                Some(EqParams::default().low_gain_db),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "mid_gain_db",
                "Mid",
                None,
                Some(EqParams::default().mid_gain_db),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "high_gain_db",
                "High",
                None,
                Some(EqParams::default().high_gain_db),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<EqParams> {
    Ok(EqParams {
        low_gain_db: required_f32(params, "low_gain_db").map_err(Error::msg)?,
        mid_gain_db: required_f32(params, "mid_gain_db").map_err(Error::msg)?,
        high_gain_db: required_f32(params, "high_gain_db").map_err(Error::msg)?,
    })
}

pub struct ThreeBandEq {
    low_gain: f32,
    mid_gain: f32,
    high_gain: f32,
    low_pass: OnePoleLowPass,
    high_pass: OnePoleHighPass,
}

impl ThreeBandEq {
    pub fn new(low_gain_db: f32, mid_gain_db: f32, high_gain_db: f32, sample_rate: f32) -> Self {
        Self {
            low_gain: db_to_lin(low_gain_db),
            mid_gain: db_to_lin(mid_gain_db),
            high_gain: db_to_lin(high_gain_db),
            low_pass: OnePoleLowPass::new(250.0, sample_rate),
            high_pass: OnePoleHighPass::new(2_000.0, sample_rate),
        }
    }
}

impl MonoProcessor for ThreeBandEq {
    fn process_sample(&mut self, input: f32) -> f32 {
        let low = self.low_pass.process(input);
        let high = self.high_pass.process(input);
        let mid = input - low - high;
        low * self.low_gain + mid * self.mid_gain + high * self.high_gain
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let params = params_from_set(params)?;
    Ok(Box::new(ThreeBandEq::new(
        params.low_gain_db,
        params.mid_gain_db,
        params.high_gain_db,
        sample_rate,
    )))
}
