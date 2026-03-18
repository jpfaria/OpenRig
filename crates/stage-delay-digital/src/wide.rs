use anyhow::{Error, Result};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{ModelAudioMode, StereoProcessor};

use crate::basic::{MAX_DELAY_MS, MAX_FEEDBACK};

pub const MODEL_ID: &str = "digital_wide";
const SMOOTH_TIME_MS: f32 = 50.0;
const DENORMAL_THRESHOLD: f32 = 1e-20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WideDelayParams {
    pub left_time_ms: f32,
    pub right_time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for WideDelayParams {
    fn default() -> Self {
        Self {
            left_time_ms: 340.0,
            right_time_ms: 510.0,
            feedback: 0.42,
            mix: 0.32,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "wide")
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Digital Wide Delay".to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "left_time_ms",
                "Left Time",
                None,
                Some(WideDelayParams::default().left_time_ms),
                1.0,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "right_time_ms",
                "Right Time",
                None,
                Some(WideDelayParams::default().right_time_ms),
                1.0,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(WideDelayParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(WideDelayParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<WideDelayParams> {
    Ok(WideDelayParams {
        left_time_ms: required_f32(params, "left_time_ms").map_err(Error::msg)?,
        right_time_ms: required_f32(params, "right_time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

pub struct WideDelay {
    params: WideDelayParams,
    left_buffer: Vec<f32>,
    right_buffer: Vec<f32>,
    write_pos: usize,
    left_delay_smoothed: f32,
    left_delay_target: f32,
    right_delay_smoothed: f32,
    right_delay_target: f32,
    smooth_coeff: f32,
}

impl WideDelay {
    pub fn new(params: WideDelayParams, sample_rate: f32) -> Self {
        let params = WideDelayParams {
            left_time_ms: params.left_time_ms.clamp(1.0, MAX_DELAY_MS),
            right_time_ms: params.right_time_ms.clamp(1.0, MAX_DELAY_MS),
            feedback: params.feedback.clamp(0.0, MAX_FEEDBACK),
            mix: params.mix.clamp(0.0, 1.0),
        };
        let max_samples = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + 2;
        Self {
            left_buffer: vec![0.0; max_samples],
            right_buffer: vec![0.0; max_samples],
            write_pos: 0,
            left_delay_smoothed: params.left_time_ms * 0.001 * sample_rate,
            left_delay_target: params.left_time_ms * 0.001 * sample_rate,
            right_delay_smoothed: params.right_time_ms * 0.001 * sample_rate,
            right_delay_target: params.right_time_ms * 0.001 * sample_rate,
            smooth_coeff: calculate_coefficient(SMOOTH_TIME_MS, sample_rate),
            params,
        }
    }
}

impl StereoProcessor for WideDelay {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.left_delay_smoothed = self.smooth_coeff.mul_add(
            self.left_delay_smoothed,
            (1.0 - self.smooth_coeff) * self.left_delay_target,
        );
        self.right_delay_smoothed = self.smooth_coeff.mul_add(
            self.right_delay_smoothed,
            (1.0 - self.smooth_coeff) * self.right_delay_target,
        );

        let delayed_left = read_delay(&self.left_buffer, self.write_pos, self.left_delay_smoothed);
        let delayed_right = read_delay(
            &self.right_buffer,
            self.write_pos,
            self.right_delay_smoothed,
        );

        self.left_buffer[self.write_pos] = sanitize(input[0] + delayed_left * self.params.feedback);
        self.right_buffer[self.write_pos] =
            sanitize(input[1] + delayed_right * self.params.feedback);
        self.write_pos = (self.write_pos + 1) % self.left_buffer.len();

        [
            (1.0 - self.params.mix).mul_add(input[0], self.params.mix * delayed_left),
            (1.0 - self.params.mix).mul_add(input[1], self.params.mix * delayed_right),
        ]
    }
}

pub fn build_stereo_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn StereoProcessor>> {
    Ok(Box::new(WideDelay::new(
        params_from_set(params)?,
        sample_rate,
    )))
}

fn sanitize(value: f32) -> f32 {
    if value.abs() < DENORMAL_THRESHOLD {
        0.0
    } else {
        value
    }
}

fn read_delay(buffer: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let clamped_delay = delay_samples.max(1.0);
    let delay_whole = clamped_delay as usize;
    let frac = delay_samples - delay_whole as f32;
    let buffer_len = buffer.len();
    let read_idx = (write_pos + buffer_len - delay_whole) % buffer_len;
    let prev_idx = (write_pos + buffer_len - delay_whole - 1) % buffer_len;
    (1.0 - frac).mul_add(buffer[read_idx], frac * buffer[prev_idx])
}

fn calculate_coefficient(smooth_time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (smooth_time_ms * 0.001 * sample_rate)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_delay_outputs_finite_values() {
        let mut delay = WideDelay::new(WideDelayParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_frame([0.2, -0.1]);
            assert!(output[0].is_finite());
            assert!(output[1].is_finite());
        }
    }
}
