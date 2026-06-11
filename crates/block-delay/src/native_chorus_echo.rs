use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, soft_saturate, DelayLine,
    MAX_DELAY_MS, MAX_FEEDBACK, MIN_DELAY_MS,
};
use crate::DelayBackendKind;

pub const MODEL_ID: &str = "chorus_echo";
pub const DISPLAY_NAME: &str = "Chorus Echo";

const SATURATION_DRIVE: f32 = 2.0;
const CHORUS_RATE_HZ: f32 = 1.6;
const CHORUS_DEPTH_MS: f32 = 4.5;
const TONE_CUTOFF_HZ: f32 = 3_200.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChorusEchoParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub depth: f32,
}

impl Default for ChorusEchoParams {
    fn default() -> Self {
        Self {
            time_ms: 350.0,
            feedback: 35.0,
            mix: 32.0,
            depth: 45.0,
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
                Some(ChorusEchoParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(ChorusEchoParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(ChorusEchoParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(ChorusEchoParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<ChorusEchoParams> {
    Ok(ChorusEchoParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
    })
}

pub struct ChorusEchoDelay {
    params: ChorusEchoParams,
    line: DelayLine,
    tone_state: f32,
    phase: f32,
}

impl ChorusEchoDelay {
    pub fn new(params: ChorusEchoParams, sample_rate: f32) -> Self {
        let params = ChorusEchoParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            depth: params.depth.clamp(0.0, 1.0),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            tone_state: 0.0,
            phase: 0.0,
        }
    }
}

impl MonoProcessor for ChorusEchoDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        use std::f32::consts::TAU;
        let sample_rate = self.line.sample_rate();
        // Internal chorus: a slow sine sweeps the delay time.
        self.phase = (self.phase + TAU * CHORUS_RATE_HZ / sample_rate) % TAU;
        let modulated_time =
            self.params.time_ms + self.phase.sin() * CHORUS_DEPTH_MS * self.params.depth;
        self.line.set_delay_ms(modulated_time);
        let delayed = self.line.read();
        let filtered = lowpass_step(&mut self.tone_state, delayed, TONE_CUTOFF_HZ, sample_rate);
        let colored = soft_saturate(filtered, SATURATION_DRIVE);
        self.line.write(input + colored * self.params.feedback);
        mix_dry_wet(input, colored, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(ChorusEchoDelay::new(
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
#[path = "native_chorus_echo_tests.rs"]
mod tests;
