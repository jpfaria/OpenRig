//! Responsibility: keeps a delay's numbers inside the range the DSP can take.

use crate::delay_line::DelayLine;

pub const MIN_DELAY_MS: f32 = 1.0;
pub const MAX_DELAY_MS: f32 = 2_000.0;
pub const MAX_FEEDBACK: f32 = 0.95;
pub const SMOOTH_TIME_MS: f32 = 50.0;
pub const DENORMAL_THRESHOLD: f32 = 1e-20;

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

/// `tanh` soft saturation with unity small-signal gain. `drive` sets the knee:
/// higher = earlier compression and more odd harmonics. Branch-free, RT-safe.
pub fn soft_saturate(input: f32, drive: f32) -> f32 {
    let drive = drive.max(1e-6);
    (input * drive).tanh() / drive
}

pub fn lowpass_step(state: &mut f32, input: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
    let cutoff_hz = cutoff_hz.clamp(20.0, (sample_rate * 0.45).max(20.0));
    let alpha = 1.0 - (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate).exp();
    *state += alpha * (input - *state);
    *state
}

pub(crate) fn calculate_coefficient(smooth_time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (smooth_time_ms * 0.001 * sample_rate)).exp()
}

pub(crate) fn read_interpolated(buffer: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let delay_whole = delay_samples as usize;
    let frac = delay_samples - delay_whole as f32;
    let buffer_len = buffer.len();
    let read_idx = (write_pos + buffer_len - delay_whole) % buffer_len;
    let prev_idx = (write_pos + buffer_len - delay_whole - 1) % buffer_len;
    (1.0 - frac).mul_add(buffer[read_idx], frac * buffer[prev_idx])
}
