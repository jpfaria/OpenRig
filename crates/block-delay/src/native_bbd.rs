use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::DelayBackendKind;
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, soft_saturate, DelayLine,
    MAX_DELAY_MS, MAX_FEEDBACK, MIN_DELAY_MS,
};

pub const MODEL_ID: &str = "bbd";
pub const DISPLAY_NAME: &str = "BBD Analog Delay";

/// Bucket-brigade reconstruction is a multi-pole low-pass — far steeper HF loss
/// per repeat than a single-pole "warm" delay, which is the BBD signature.
const SATURATION_DRIVE: f32 = 2.2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BbdParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub tone: f32,
}

impl Default for BbdParams {
    fn default() -> Self {
        Self {
            time_ms: 320.0,
            feedback: 35.0,
            mix: 30.0,
            tone: 40.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(BbdParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(BbdParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(BbdParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "tone",
                "Tone",
                None,
                Some(BbdParams::default().tone),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<BbdParams> {
    Ok(BbdParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        tone: required_f32(params, "tone").map_err(Error::msg)? / 100.0,
    })
}

pub struct BbdDelay {
    params: BbdParams,
    line: DelayLine,
    /// Two-pole reconstruction low-pass state (the steep BBD HF loss).
    lp1: f32,
    lp2: f32,
}

impl BbdDelay {
    pub fn new(params: BbdParams, sample_rate: f32) -> Self {
        let params = BbdParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            tone: params.tone.clamp(0.0, 1.0),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            lp1: 0.0,
            lp2: 0.0,
        }
    }

    fn cutoff_hz(&self) -> f32 {
        let min = 1_200.0;
        let max = 4_500.0;
        min + (max - min) * self.params.tone
    }
}

impl MonoProcessor for BbdDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sample_rate = self.line.sample_rate();
        let cutoff = self.cutoff_hz();
        let delayed = self.line.read();
        // Two cascaded poles → the steep per-repeat HF roll-off of a BBD line.
        let s1 = lowpass_step(&mut self.lp1, delayed, cutoff, sample_rate);
        let s2 = lowpass_step(&mut self.lp2, s1, cutoff, sample_rate);
        // Analog saturation of the bucket-brigade signal.
        let colored = soft_saturate(s2, SATURATION_DRIVE);
        self.line.write(input + colored * self.params.feedback);
        mix_dry_wet(input, colored, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(BbdDelay::new(params_from_set(params)?, sample_rate)))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    build_dual_mono_delay_processor(layout, || build_mono_processor(params, sample_rate))
}

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: DelayBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
#[path = "native_bbd_tests.rs"]
mod tests;
