use anyhow::{Error, Result};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{ModelAudioMode, MonoProcessor};

use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, mix_dry_wet, DelayLine, MAX_DELAY_MS,
    MAX_FEEDBACK, MIN_DELAY_MS,
};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "modulated_delay";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModulatedDelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub rate_hz: f32,
    pub depth: f32,
}

impl Default for ModulatedDelayParams {
    fn default() -> Self {
        Self {
            time_ms: 410.0,
            feedback: 0.38,
            mix: 0.30,
            rate_hz: 0.8,
            depth: 0.35,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    model == MODEL_ID
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Modulated Delay".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(ModulatedDelayParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(ModulatedDelayParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(ModulatedDelayParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(ModulatedDelayParams::default().rate_hz),
                0.05,
                8.0,
                0.01,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(ModulatedDelayParams::default().depth),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<ModulatedDelayParams> {
    Ok(ModulatedDelayParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)?,
    })
}

pub struct ModulatedDelay {
    params: ModulatedDelayParams,
    line: DelayLine,
    phase: f32,
}

impl ModulatedDelay {
    pub fn new(params: ModulatedDelayParams, sample_rate: f32) -> Self {
        let params = ModulatedDelayParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            rate_hz: params.rate_hz.clamp(0.05, 8.0),
            depth: params.depth.clamp(0.0, 1.0),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            phase: 0.0,
        }
    }

    fn modulation_amount_ms(&self) -> f32 {
        (self.params.time_ms * 0.35).min(25.0) * self.params.depth
    }
}

impl MonoProcessor for ModulatedDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sample_rate = self.line.sample_rate();
        self.phase = wrap_phase(self.phase + TAU * self.params.rate_hz / sample_rate);
        let modulated_time =
            self.params.time_ms + self.phase.sin() * self.modulation_amount_ms();
        self.line.set_delay_ms(modulated_time);
        let delayed = self.line.read();
        self.line
            .write(input + delayed * self.params.feedback);
        mix_dry_wet(input, delayed, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(ModulatedDelay::new(
        params_from_set(params)?,
        sample_rate,
    )))
}

fn wrap_phase(phase: f32) -> f32 {
    if phase >= TAU {
        phase - TAU
    } else {
        phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stage_core::MonoProcessor;

    #[test]
    fn modulated_delay_outputs_finite_values() {
        let mut delay = ModulatedDelay::new(ModulatedDelayParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }
}
