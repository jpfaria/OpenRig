//! Reference-free tone descriptors for the Tone Doctor diagnosis (#791).
//!
//! Pure, self-contained DSP: in = a rendered buffer, out = a small set of
//! scalar descriptors plus a symptom classification. No smoothing, no state,
//! no UI — the caller owns everything else. Given the same samples it always
//! returns the same descriptors (deterministic, invariant #9).
//!
//! The descriptors answer "is this tone healthy?" without a reference:
//!
//! - **fizz / harsh** — too much energy in the presence band (~3–8 kHz)
//!   relative to the note body (~200 Hz–2 kHz).
//! - **mud / boxy** — too much energy in the low-mid band (~160–500 Hz)
//!   relative to the whole signal.
//! - **clipping** — samples pinned at the ±1.0 rail.
//!
//! Band energy is measured with a Welch-averaged power spectrum (Hann window,
//! 50 % overlap) so a multi-second take is summarised by one stable estimate.

use rustfft::{num_complex::Complex, FftPlanner};

/// FFT size for the Welch power-spectrum estimate.
const FFT_SIZE: usize = 4096;
/// Hop between successive Welch frames (50 % overlap).
const HOP_SIZE: usize = 2048;

/// Amplitude at/above which a sample counts as clipped.
const CLIP_THRESHOLD: f32 = 0.999;

/// Presence / "fizz" band edges (Hz).
const FIZZ_LO_HZ: f32 = 3_000.0;
const FIZZ_HI_HZ: f32 = 8_000.0;
/// Note-body band edges (Hz) — the reference the fizz band is judged against.
const BODY_LO_HZ: f32 = 200.0;
const BODY_HI_HZ: f32 = 2_000.0;
/// Low-mid / "mud" band edges (Hz).
const MUD_LO_HZ: f32 = 160.0;
const MUD_HI_HZ: f32 = 500.0;

/// A reference-free description of a rendered buffer's tonal health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneDescriptors {
    /// RMS level of the whole buffer, dBFS.
    pub rms_dbfs: f32,
    /// Peak absolute sample, dBFS.
    pub peak_dbfs: f32,
    /// Crest factor (peak − RMS), dB. Low values signal heavy compression /
    /// clipping; a clean pluck is ~12–20 dB.
    pub crest_db: f32,
    /// Fraction of samples pinned at the rail, 0.0..1.0.
    pub clip_fraction: f32,
    /// Presence-band power divided by body-band power (linear). Higher = fizzier.
    pub fizz_ratio: f32,
    /// Low-mid power divided by total power (linear). Higher = muddier.
    pub mud_ratio: f32,
}

/// The dominant tonal problem in a buffer, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symptom {
    Ok,
    Fizz,
    Mud,
    Clipping,
}

/// Thresholds separating a healthy tone from each symptom. Exposed so the
/// diagnosis layer reuses the same cut-offs when it walks the growth curve.
///
/// PROVISIONAL: these separate the descriptor's clean value (≈0 for `fizz`/
/// `mud`, 0 for `clip`) from a signal carrying real presence-band / low-mid /
/// rail-pinned content, and are set conservatively so a clean signal never
/// trips them. The *musical* boundary between wanted and unwanted colour
/// (a fuzz is meant to be buzzy) needs real recordings and the player's ear to
/// calibrate — that tuning is deferred (#791). `fizz_ratio` = presence-band
/// power as a fraction of body-band power, so 0.05 means "presence ≥ 5 % of
/// the note body".
pub const FIZZ_RATIO_LIMIT: f32 = 0.05;
pub const MUD_RATIO_LIMIT: f32 = 0.55;
pub const CLIP_FRACTION_LIMIT: f32 = 0.001;

impl ToneDescriptors {
    /// Classify the dominant symptom. Clipping wins over spectral tilt because
    /// it is the most audible failure; otherwise the band with the largest
    /// relative excess is reported.
    pub fn symptom(&self) -> Symptom {
        if self.clip_fraction > CLIP_FRACTION_LIMIT {
            return Symptom::Clipping;
        }
        let fizz_excess = self.fizz_ratio - FIZZ_RATIO_LIMIT;
        let mud_excess = self.mud_ratio - MUD_RATIO_LIMIT;
        if fizz_excess <= 0.0 && mud_excess <= 0.0 {
            return Symptom::Ok;
        }
        // Normalise each excess by its limit so the two bands compare fairly.
        if fizz_excess / FIZZ_RATIO_LIMIT >= mud_excess / MUD_RATIO_LIMIT {
            Symptom::Fizz
        } else {
            Symptom::Mud
        }
    }
}

/// Analyse a stereo buffer by collapsing to mono (mean of L/R) first.
pub fn analyze(samples: &[[f32; 2]], sample_rate: f32) -> ToneDescriptors {
    let mono: Vec<f32> = samples.iter().map(|f| 0.5 * (f[0] + f[1])).collect();
    analyze_mono(&mono, sample_rate)
}

/// Analyse a mono buffer.
pub fn analyze_mono(samples: &[f32], sample_rate: f32) -> ToneDescriptors {
    if samples.is_empty() {
        return ToneDescriptors {
            rms_dbfs: f32::NEG_INFINITY,
            peak_dbfs: f32::NEG_INFINITY,
            crest_db: 0.0,
            clip_fraction: 0.0,
            fizz_ratio: 0.0,
            mud_ratio: 0.0,
        };
    }

    let mut peak = 0.0_f32;
    let mut sum_sq = 0.0_f64;
    let mut clipped = 0usize;
    for &s in samples {
        let a = s.abs();
        if a > peak {
            peak = a;
        }
        if a >= CLIP_THRESHOLD {
            clipped += 1;
        }
        sum_sq += (s as f64) * (s as f64);
    }
    let rms = (sum_sq / samples.len() as f64).sqrt() as f32;

    let power = welch_power_spectrum(samples);
    let fizz = band_power(&power, sample_rate, FIZZ_LO_HZ, FIZZ_HI_HZ);
    let body = band_power(&power, sample_rate, BODY_LO_HZ, BODY_HI_HZ);
    let mud = band_power(&power, sample_rate, MUD_LO_HZ, MUD_HI_HZ);
    let total: f32 = power.iter().sum();

    let peak_dbfs = lin_to_dbfs(peak);
    let rms_dbfs = lin_to_dbfs(rms);

    ToneDescriptors {
        rms_dbfs,
        peak_dbfs,
        crest_db: (peak_dbfs - rms_dbfs).max(0.0),
        clip_fraction: clipped as f32 / samples.len() as f32,
        fizz_ratio: safe_ratio(fizz, body),
        mud_ratio: safe_ratio(mud, total),
    }
}

/// Linear amplitude → dBFS, with a floor so silence is finite-ish.
fn lin_to_dbfs(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// `num / den`, returning 0.0 when the denominator is negligible (silence).
fn safe_ratio(num: f32, den: f32) -> f32 {
    if den <= 1e-12 {
        0.0
    } else {
        num / den
    }
}

/// Sum the FFT power in `[lo_hz, hi_hz)` from a Welch power spectrum.
fn band_power(power: &[f32], sample_rate: f32, lo_hz: f32, hi_hz: f32) -> f32 {
    let bin_hz = sample_rate / FFT_SIZE as f32;
    let lo = (lo_hz / bin_hz).floor() as usize;
    let hi = ((hi_hz / bin_hz).ceil() as usize).min(power.len());
    power.get(lo..hi).map(|s| s.iter().sum()).unwrap_or(0.0)
}

/// Welch-averaged power spectrum: window the signal into overlapping Hann
/// frames, FFT each, and average the per-bin power. Returns the lower half
/// (positive frequencies) so bin `k` maps to `k * sample_rate / FFT_SIZE` Hz.
fn welch_power_spectrum(samples: &[f32]) -> Vec<f32> {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let hann: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos()))
        .collect();
    let mut power = vec![0.0_f32; FFT_SIZE / 2];
    let mut frames = 0usize;
    let mut scratch = vec![Complex::new(0.0, 0.0); FFT_SIZE];
    let mut start = 0;
    while start + FFT_SIZE <= samples.len() {
        for i in 0..FFT_SIZE {
            scratch[i] = Complex::new(samples[start + i] * hann[i], 0.0);
        }
        fft.process(&mut scratch);
        for (p, c) in power.iter_mut().zip(scratch.iter()) {
            *p += c.norm_sqr();
        }
        frames += 1;
        start += HOP_SIZE;
    }
    if frames > 1 {
        for p in power.iter_mut() {
            *p /= frames as f32;
        }
    }
    power
}

#[cfg(test)]
#[path = "tone_descriptors_tests.rs"]
mod tests;
