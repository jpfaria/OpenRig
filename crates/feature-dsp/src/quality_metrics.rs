//! Objective audio-quality metrics for the Tone Doctor report (#791, Layer 3;
//! was #609).
//!
//! Pure, deterministic DSP driven by a synthetic test-signal battery: each
//! metric has an unambiguous expected value for a known input, so the maths is
//! validated in CI without hardware. The caller (engine side) pushes each
//! battery signal through the deterministic offline render and hands the output
//! back here for measurement.
//!
//! The metrics map onto the audio invariants in `CLAUDE.md`:
//!
//! - **THD+N** — a single 1 kHz tone in; harmonic + noise energy as a fraction
//!   of the fundamental. Invariant #2 (audio quality).
//! - **Noise floor** — silence in; residual output RMS in dBFS. Invariant #2.
//! - **Peak / RMS / clipping** — level and rail-pinning over a render.
//! - **Dynamic range** — peak-to-RMS spread, dB.

use rustfft::{num_complex::Complex, FftPlanner};

/// FFT size for the single-tone THD+N estimate. A whole number of 1 kHz cycles
/// fits (48 kHz / 1 kHz = 48), so leakage is negligible without a window.
const THD_FFT_SIZE: usize = 48_000;

/// A deterministic test signal from the battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatterySignal {
    /// A 1 kHz sine at −6 dBFS — the THD+N probe.
    Sine1k,
    /// Digital silence — the noise-floor probe.
    Silence,
}

impl BatterySignal {
    /// Render `secs` seconds of this signal at `sample_rate`, as mono samples.
    pub fn generate(self, secs: f32, sample_rate: f32) -> Vec<f32> {
        let n = (secs * sample_rate) as usize;
        match self {
            BatterySignal::Sine1k => (0..n)
                .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / sample_rate).sin())
                .collect(),
            BatterySignal::Silence => vec![0.0; n],
        }
    }
}

/// The objective quality report for one chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityMetrics {
    /// Total harmonic distortion + noise, as a linear fraction of the
    /// fundamental (0.01 = 1 %). Measured from the 1 kHz probe.
    pub thd_n: f32,
    /// Residual output RMS with silence in, dBFS. Lower is quieter.
    pub noise_floor_dbfs: f32,
    /// Peak sample of the 1 kHz probe render, dBFS.
    pub peak_dbfs: f32,
    /// RMS of the 1 kHz probe render, dBFS.
    pub rms_dbfs: f32,
    /// Peak − RMS spread of the 1 kHz probe render, dB.
    pub dynamic_range_db: f32,
    /// Fraction of the 1 kHz probe render pinned at the ±1.0 rail.
    pub clip_fraction: f32,
}

/// Total harmonic distortion + noise of a rendered single tone, as a linear
/// fraction of the fundamental. A pure tone → ~0; a distorted one → larger.
pub fn thd_n(output: &[f32], fundamental_hz: f32, sample_rate: f32) -> f32 {
    if output.len() < THD_FFT_SIZE {
        return 0.0;
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(THD_FFT_SIZE);
    // Take the most recent window so any startup transient has settled.
    let start = output.len() - THD_FFT_SIZE;
    let mut buf: Vec<Complex<f32>> = output[start..start + THD_FFT_SIZE]
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .collect();
    fft.process(&mut buf);

    let bin_hz = sample_rate / THD_FFT_SIZE as f32;
    let fundamental_bin = (fundamental_hz / bin_hz).round() as usize;
    // Guard a ±2-bin skirt around the fundamental as "signal"; everything else
    // in the positive-frequency half is distortion + noise.
    let half = THD_FFT_SIZE / 2;
    let mut fundamental_power = 0.0_f64;
    let mut rest_power = 0.0_f64;
    for (k, c) in buf.iter().take(half).enumerate() {
        let p = c.norm_sqr() as f64;
        if k.abs_diff(fundamental_bin) <= 2 {
            fundamental_power += p;
        } else {
            rest_power += p;
        }
    }
    if fundamental_power <= 0.0 {
        return 0.0;
    }
    (rest_power / fundamental_power).sqrt() as f32
}

/// RMS of a buffer in dBFS (−∞ floored to a finite −180 dB for silence).
pub fn rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -180.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
    lin_to_dbfs(rms)
}

/// Peak of a buffer in dBFS.
pub fn peak_dbfs(samples: &[f32]) -> f32 {
    let peak = samples.iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
    lin_to_dbfs(peak)
}

/// Fraction of samples pinned at/above the ±1.0 rail.
pub fn clip_fraction(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let clipped = samples.iter().filter(|&&s| s.abs() >= 0.999).count();
    clipped as f32 / samples.len() as f32
}

fn lin_to_dbfs(x: f32) -> f32 {
    if x <= 1e-9 {
        return -180.0;
    }
    20.0 * x.log10()
}

/// Assemble the full report from the two rendered probe outputs.
pub fn assemble(
    sine_output: &[f32],
    silence_output: &[f32],
    sample_rate: f32,
) -> QualityMetrics {
    let peak = peak_dbfs(sine_output);
    let rms = rms_dbfs(sine_output);
    QualityMetrics {
        thd_n: thd_n(sine_output, 1_000.0, sample_rate),
        noise_floor_dbfs: rms_dbfs(silence_output),
        peak_dbfs: peak,
        rms_dbfs: rms,
        dynamic_range_db: (peak - rms).max(0.0),
        clip_fraction: clip_fraction(sine_output),
    }
}

#[cfg(test)]
#[path = "quality_metrics_tests.rs"]
mod tests;
