//! Responsibility: conditions a loaded impulse response for the runtime sample rate.
//!
//! Split out of `lib.rs` (#873). Truncation with a cosine fade keeps a long
//! tail from costing more than it is worth; Lanczos resampling matches the
//! file's rate to the stream's. Both run at load time, never on the audio
//! thread.

/// Maximum IR length in samples at the file's native sample rate.
/// Longer tails are truncated with a cosine fade-out.
/// 8192 samples ≈ 170ms at 48kHz — more than enough for cabs and body IRs.
pub(crate) const MAX_IR_SAMPLES: usize = 8192;

/// Fade-out length in samples applied when truncating.
pub(crate) const FADE_OUT_SAMPLES: usize = 512;

pub(crate) fn truncate_with_fade(mut samples: Vec<f32>, path: &str) -> Vec<f32> {
    if samples.len() <= MAX_IR_SAMPLES {
        return samples;
    }
    log::info!(
        "truncating IR '{}' from {} to {} samples with {}‑sample fade‑out",
        path,
        samples.len(),
        MAX_IR_SAMPLES,
        FADE_OUT_SAMPLES
    );
    samples.truncate(MAX_IR_SAMPLES);
    let fade_start = MAX_IR_SAMPLES.saturating_sub(FADE_OUT_SAMPLES);
    for (i, sample) in samples.iter_mut().enumerate().skip(fade_start) {
        let t = (i - fade_start) as f32 / FADE_OUT_SAMPLES as f32;
        let gain = 0.5 * (1.0 + (std::f32::consts::PI * t).cos()); // cosine fade
        *sample *= gain;
    }
    samples
}

pub(crate) fn resample_if_needed(
    samples: Vec<f32>,
    ir_rate: u32,
    runtime_rate: f32,
    path: &str,
) -> Vec<f32> {
    let runtime_rate = runtime_rate.round() as u32;
    if runtime_rate == 0 || ir_rate == runtime_rate {
        return samples;
    }
    log::info!(
        "resampling IR '{}' from {}Hz to {}Hz ({} samples)",
        path,
        ir_rate,
        runtime_rate,
        samples.len()
    );
    let ratio = runtime_rate as f64 / ir_rate as f64;
    let new_len = (samples.len() as f64 * ratio).round() as usize;
    if new_len == 0 {
        return vec![0.0];
    }
    // Windowed sinc interpolation (Lanczos kernel, a=4)
    const SINC_HALF_WIDTH: usize = 4;
    let mut resampled = Vec::with_capacity(new_len);
    for i in 0..new_len {
        let src_pos = i as f64 / ratio;
        let center = src_pos.floor() as i64;
        let frac = src_pos - center as f64;
        let mut sum = 0.0f64;
        let mut weight_sum = 0.0f64;
        for j in -(SINC_HALF_WIDTH as i64)..=(SINC_HALF_WIDTH as i64) {
            let idx = center + j;
            if idx < 0 || idx >= samples.len() as i64 {
                continue;
            }
            let x = frac - j as f64;
            let w = lanczos_kernel(x, SINC_HALF_WIDTH as f64);
            sum += samples[idx as usize] as f64 * w;
            weight_sum += w;
        }
        let value = if weight_sum.abs() > 1e-10 {
            sum / weight_sum
        } else {
            0.0
        };
        resampled.push(value as f32);
    }
    resampled
}

pub(crate) fn lanczos_kernel(x: f64, a: f64) -> f64 {
    if x.abs() < 1e-10 {
        return 1.0;
    }
    if x.abs() >= a {
        return 0.0;
    }
    let pi_x = std::f64::consts::PI * x;
    (a * pi_x.sin() * (pi_x / a).sin()) / (pi_x * pi_x)
}
