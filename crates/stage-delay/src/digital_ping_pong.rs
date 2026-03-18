use anyhow::{Error, Result};
use stage_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use stage_core::{ModelAudioMode, StereoProcessor};

use crate::digital_basic::{MAX_DELAY_MS, MAX_FEEDBACK};

pub const MODEL_ID: &str = "digital_ping_pong";
const SMOOTH_TIME_MS: f32 = 50.0;
const DENORMAL_THRESHOLD: f32 = 1e-20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingPongDelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for PingPongDelayParams {
    fn default() -> Self {
        Self {
            time_ms: 420.0,
            feedback: 0.55,
            mix: 0.35,
        }
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(model, MODEL_ID | "ping_pong")
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Digital Ping Pong Delay".to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(PingPongDelayParams::default().time_ms),
                1.0,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(PingPongDelayParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(PingPongDelayParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<PingPongDelayParams> {
    Ok(PingPongDelayParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

pub struct PingPongDelay {
    params: PingPongDelayParams,
    left_buffer: Vec<f32>,
    right_buffer: Vec<f32>,
    write_pos: usize,
    delay_samples_smoothed: f32,
    delay_samples_target: f32,
    smooth_coeff: f32,
}

impl PingPongDelay {
    pub fn new(params: PingPongDelayParams, sample_rate: f32) -> Self {
        let params = PingPongDelayParams {
            time_ms: params.time_ms.clamp(1.0, MAX_DELAY_MS),
            feedback: params.feedback.clamp(0.0, MAX_FEEDBACK),
            mix: params.mix.clamp(0.0, 1.0),
        };
        let max_samples = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + 2;
        let delay_samples = params.time_ms * 0.001 * sample_rate;
        Self {
            params,
            left_buffer: vec![0.0; max_samples],
            right_buffer: vec![0.0; max_samples],
            write_pos: 0,
            delay_samples_smoothed: delay_samples,
            delay_samples_target: delay_samples,
            smooth_coeff: calculate_coefficient(SMOOTH_TIME_MS, sample_rate),
        }
    }
}

impl StereoProcessor for PingPongDelay {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.delay_samples_smoothed = self.smooth_coeff.mul_add(
            self.delay_samples_smoothed,
            (1.0 - self.smooth_coeff) * self.delay_samples_target,
        );

        let delayed_left = read_delay(
            &self.left_buffer,
            self.write_pos,
            self.delay_samples_smoothed,
        );
        let delayed_right = read_delay(
            &self.right_buffer,
            self.write_pos,
            self.delay_samples_smoothed,
        );
        let input_mono = (input[0] + input[1]) * 0.5;

        let write_left = input_mono + delayed_right * self.params.feedback;
        let write_right = delayed_left * self.params.feedback;
        self.left_buffer[self.write_pos] = sanitize(write_left);
        self.right_buffer[self.write_pos] = sanitize(write_right);
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
    Ok(Box::new(PingPongDelay::new(
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
    let frac = clamped_delay - delay_whole as f32;
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
    fn ping_pong_outputs_finite_values() {
        let mut delay = PingPongDelay::new(PingPongDelayParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_frame([0.2, 0.2]);
            assert!(output[0].is_finite());
            assert!(output[1].is_finite());
        }
    }
}
