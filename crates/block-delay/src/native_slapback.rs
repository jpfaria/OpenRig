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

pub const MODEL_ID: &str = "slapback";
pub const DISPLAY_NAME: &str = "Slapback Delay";

/// Tape-style high-frequency roll-off on the repeat — what makes a slapback
/// sound analog instead of a pristine digital tap.
const TONE_CUTOFF_HZ: f32 = 2_800.0;
/// Gentle analog warmth on the repeat.
const SATURATION_DRIVE: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlapbackParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for SlapbackParams {
    fn default() -> Self {
        Self {
            time_ms: 110.0,
            feedback: 18.0,
            mix: 28.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Slapback Delay".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(SlapbackParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(SlapbackParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(SlapbackParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<SlapbackParams> {
    Ok(SlapbackParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct SlapbackDelay {
    params: SlapbackParams,
    line: DelayLine,
    tone_state: f32,
}

impl SlapbackDelay {
    pub fn new(params: SlapbackParams, sample_rate: f32) -> Self {
        let params = SlapbackParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            tone_state: 0.0,
        }
    }
}

impl MonoProcessor for SlapbackDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sample_rate = self.line.sample_rate();
        let delayed = self.line.read();
        // Tape-style darkening + analog warmth on the repeat — the slap's identity.
        let darkened = lowpass_step(&mut self.tone_state, delayed, TONE_CUTOFF_HZ, sample_rate);
        let colored = soft_saturate(darkened, SATURATION_DRIVE);
        self.line.write(input + colored * self.params.feedback);
        mix_dry_wet(input, colored, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(SlapbackDelay::new(
        params_from_set(params)?,
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
#[path = "native_slapback_tests.rs"]
mod tests;
