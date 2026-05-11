use anyhow::Result;
use block_core::{MonoProcessor, StereoProcessor};

pub const MIN_DELAY_MS: f32 = 1.0;
pub const MAX_DELAY_MS: f32 = 2_000.0;
pub const MAX_FEEDBACK: f32 = 0.95;
const SMOOTH_TIME_MS: f32 = 50.0;
const DENORMAL_THRESHOLD: f32 = 1e-20;

pub struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples_smoothed: f32,
    delay_samples_target: f32,
    smooth_coeff: f32,
    sample_rate: f32,
}

impl DelayLine {
    pub fn new(initial_time_ms: f32, sample_rate: f32) -> Self {
        let max_samples = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + 2;
        let delay_samples = clamp_time_ms(initial_time_ms) * 0.001 * sample_rate;
        Self {
            buffer: vec![0.0; max_samples],
            write_pos: 0,
            delay_samples_smoothed: delay_samples,
            delay_samples_target: delay_samples,
            smooth_coeff: calculate_coefficient(SMOOTH_TIME_MS, sample_rate),
            sample_rate,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn set_delay_ms(&mut self, time_ms: f32) {
        self.delay_samples_target = clamp_time_ms(time_ms) * 0.001 * self.sample_rate;
    }

    pub fn read(&mut self) -> f32 {
        self.delay_samples_smoothed = self.smooth_coeff.mul_add(
            self.delay_samples_smoothed,
            (1.0 - self.smooth_coeff) * self.delay_samples_target,
        );
        read_interpolated(
            &self.buffer,
            self.write_pos,
            self.delay_samples_smoothed.max(1.0),
        )
    }

    pub fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sanitize(sample);
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
    }
}

pub struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

impl DualMonoProcessor {
    pub fn new(left: Box<dyn MonoProcessor>, right: Box<dyn MonoProcessor>) -> Self {
        Self { left, right }
    }
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

pub fn build_dual_mono_from_builder<F>(builder: F) -> Result<Box<dyn StereoProcessor>>
where
    F: Fn() -> Result<Box<dyn MonoProcessor>>,
{
    let left = builder()?;
    let right = builder()?;
    Ok(Box::new(DualMonoProcessor::new(left, right)))
}

pub fn process_simple_delay(line: &mut DelayLine, input: f32, feedback: f32, mix: f32) -> f32 {
    let delayed = line.read();
    line.write(input + delayed * clamp_feedback(feedback));
    mix_dry_wet(input, delayed, mix)
}

pub fn sanitize(value: f32) -> f32 {
    if value.abs() < DENORMAL_THRESHOLD {
        0.0
    } else {
        value
    }
}

pub fn clamp_feedback(feedback: f32) -> f32 {
    feedback.clamp(0.0, MAX_FEEDBACK)
}

pub fn clamp_mix(mix: f32) -> f32 {
    mix.clamp(0.0, 1.0)
}

pub fn clamp_time_ms(time_ms: f32) -> f32 {
    time_ms.clamp(MIN_DELAY_MS, MAX_DELAY_MS)
}

pub fn mix_dry_wet(dry: f32, wet: f32, mix: f32) -> f32 {
    (1.0 - clamp_mix(mix)).mul_add(dry, clamp_mix(mix) * wet)
}

pub fn lowpass_step(state: &mut f32, input: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let cutoff_hz = cutoff_hz.clamp(20.0, (sample_rate * 0.45).max(20.0));
    let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate).exp();
    *state += alpha * (input - *state);
    *state
}

fn calculate_coefficient(smooth_time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (smooth_time_ms * 0.001 * sample_rate)).exp()
}

fn read_interpolated(buffer: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let delay_whole = delay_samples as usize;
    let frac = delay_samples - delay_whole as f32;
    let buffer_len = buffer.len();
    let read_idx = (write_pos + buffer_len - delay_whole) % buffer_len;
    let prev_idx = (write_pos + buffer_len - delay_whole - 1) % buffer_len;
    (1.0 - frac).mul_add(buffer[read_idx], frac * buffer[prev_idx])
}
