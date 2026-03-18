use anyhow::{Error, Result};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::MonoProcessor;

pub const MODEL_ID: &str = "digital_basic";
pub const MAX_DELAY_MS: f32 = 2_000.0;
pub const MAX_FEEDBACK: f32 = 0.95;
const SMOOTH_TIME_MS: f32 = 50.0;
const DENORMAL_THRESHOLD: f32 = 1e-20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            time_ms: 380.0,
            feedback: 0.35,
            mix: 0.3,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(
        model,
        MODEL_ID | "native_digital" | "rust_style_digital" | "digital"
    )
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Digital Basic Delay".to_string(),
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(DelayParams::default().time_ms),
                0.0,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(DelayParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(DelayParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<DelayParams> {
    Ok(DelayParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

#[derive(Debug, Clone)]
pub struct BasicDelay {
    params: DelayParams,
    buffer: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,
    delay_samples_smoothed: f32,
    delay_samples_target: f32,
    smooth_coeff: f32,
}

impl BasicDelay {
    pub fn new(params: DelayParams, sample_rate: f32) -> Self {
        let params = DelayParams {
            time_ms: params.time_ms.clamp(0.0, MAX_DELAY_MS),
            feedback: params.feedback.clamp(0.0, MAX_FEEDBACK),
            mix: params.mix.clamp(0.0, 1.0),
        };
        let max_samples = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + 2;
        let delay_samples = params.time_ms * 0.001 * sample_rate;
        Self {
            params,
            buffer: vec![0.0; max_samples],
            write_pos: 0,
            sample_rate,
            delay_samples_smoothed: delay_samples,
            delay_samples_target: delay_samples,
            smooth_coeff: calculate_coefficient(SMOOTH_TIME_MS, sample_rate),
        }
    }
    pub fn params(&self) -> DelayParams {
        self.params
    }
    pub fn set_time_ms(&mut self, time_ms: f32) {
        self.params.time_ms = time_ms.clamp(0.0, MAX_DELAY_MS);
        self.delay_samples_target = self.params.time_ms * 0.001 * self.sample_rate;
    }
    pub fn set_feedback(&mut self, feedback: f32) {
        self.params.feedback = feedback.clamp(0.0, MAX_FEEDBACK);
    }
    pub fn set_mix(&mut self, mix: f32) {
        self.params.mix = mix.clamp(0.0, 1.0);
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(BasicDelay::new(
        params_from_set(params)?,
        sample_rate,
    )))
}

impl MonoProcessor for BasicDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.delay_samples_smoothed = self.smooth_coeff.mul_add(
            self.delay_samples_smoothed,
            (1.0 - self.smooth_coeff) * self.delay_samples_target,
        );
        let buffer_len = self.buffer.len();
        let clamped_delay = self.delay_samples_smoothed.max(1.0);
        let delay_whole = clamped_delay as usize;
        let frac = clamped_delay - delay_whole as f32;
        let read_idx = (self.write_pos + buffer_len - delay_whole) % buffer_len;
        let prev_idx = (self.write_pos + buffer_len - delay_whole - 1) % buffer_len;
        let delayed = (1.0 - frac).mul_add(self.buffer[read_idx], frac * self.buffer[prev_idx]);
        let write_value = self.params.feedback.mul_add(delayed, input);
        self.buffer[self.write_pos] = if write_value.abs() < DENORMAL_THRESHOLD {
            0.0
        } else {
            write_value
        };
        self.write_pos = (self.write_pos + 1) % buffer_len;
        (1.0 - self.params.mix).mul_add(input, self.params.mix * delayed)
    }
}

fn calculate_coefficient(smooth_time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (smooth_time_ms * 0.001 * sample_rate)).exp()
}
#[cfg(test)]
mod tests {
    use super::*;
    use stage_core::MonoProcessor;

    #[test]
    fn basic_delay_outputs_finite_values() {
        let mut delay = BasicDelay::new(DelayParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }
}
