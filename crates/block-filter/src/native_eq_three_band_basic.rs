use anyhow::{Error, Result};
use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{db_to_lin, ModelAudioMode, MonoProcessor, OnePoleHighPass, OnePoleLowPass};

pub const MODEL_ID: &str = "eq_three_band_basic";
pub const DISPLAY_NAME: &str = "Three Band EQ";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqParams {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl Default for EqParams {
    fn default() -> Self {
        Self {
            low: 50.0,
            mid: 50.0,
            high: 50.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "filter".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "low",
                "Low",
                None,
                Some(EqParams::default().low),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mid",
                "Mid",
                None,
                Some(EqParams::default().mid),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "high",
                "High",
                None,
                Some(EqParams::default().high),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<EqParams> {
    let low_pct = required_f32(params, "low").map_err(Error::msg)?;
    let mid_pct = required_f32(params, "mid").map_err(Error::msg)?;
    let high_pct = required_f32(params, "high").map_err(Error::msg)?;
    Ok(EqParams {
        low: -24.0 + (low_pct / 100.0) * 48.0,
        mid: -24.0 + (mid_pct / 100.0) * 48.0,
        high: -24.0 + (high_pct / 100.0) * 48.0,
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
        params.low,
        params.mid,
        params.high,
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
        block_core::AudioChannelLayout::Stereo => anyhow::bail!(
            "eq model '{}' is mono-only and cannot build native stereo processing",
            MODEL_ID
        ),
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: FilterBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
