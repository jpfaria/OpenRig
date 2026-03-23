use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::DelayBackendKind;
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, DelayLine, MAX_DELAY_MS,
    MAX_FEEDBACK, MIN_DELAY_MS,
};

pub const MODEL_ID: &str = "analog_warm";
pub const DISPLAY_NAME: &str = "Analog Warm Delay";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalogWarmParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub tone: f32,
}

impl Default for AnalogWarmParams {
    fn default() -> Self {
        Self {
            time_ms: 360.0,
            feedback: 0.38,
            mix: 0.30,
            tone: 0.45,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Analog Warm Delay".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(AnalogWarmParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(AnalogWarmParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(AnalogWarmParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "tone",
                "Tone",
                None,
                Some(AnalogWarmParams::default().tone),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<AnalogWarmParams> {
    Ok(AnalogWarmParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
        tone: required_f32(params, "tone").map_err(Error::msg)?,
    })
}

pub struct AnalogWarmDelay {
    params: AnalogWarmParams,
    line: DelayLine,
    tone_state: f32,
}

impl AnalogWarmDelay {
    pub fn new(params: AnalogWarmParams, sample_rate: f32) -> Self {
        let params = AnalogWarmParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            tone: params.tone.clamp(0.0, 1.0),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            tone_state: 0.0,
        }
    }

    fn cutoff_hz(&self) -> f32 {
        let sample_rate = self.line.sample_rate();
        let min_cutoff = 450.0;
        let max_cutoff = (sample_rate * 0.35).min(8_000.0).max(min_cutoff);
        min_cutoff + (max_cutoff - min_cutoff) * self.params.tone
    }
}

impl MonoProcessor for AnalogWarmDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let delayed = self.line.read();
        let cutoff_hz = self.cutoff_hz();
        let sample_rate = self.line.sample_rate();
        let filtered = lowpass_step(&mut self.tone_state, delayed, cutoff_hz, sample_rate);
        self.line.write(input + filtered * self.params.feedback);
        mix_dry_wet(input, filtered, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(AnalogWarmDelay::new(
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
    brand: "",
    backend_kind: DelayBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::MonoProcessor;

    #[test]
    fn analog_warm_outputs_finite_values() {
        let mut delay = AnalogWarmDelay::new(AnalogWarmParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }
}
