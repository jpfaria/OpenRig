//! Deterministic measurement probes for proving each native delay model
//! fulfils its proposal. Test-only: never linked into the audio path.
//!
//! All signal generators are deterministic (seeded LCG, no `rand`) so the
//! characterization tests stay within the numeric-determinism invariant.

use block_core::MonoProcessor;
use realfft::RealFftPlanner;

/// Drive a mono processor sample-by-sample over `input`, returning the output.
pub fn render_mono<P: MonoProcessor>(processor: &mut P, input: &[f32]) -> Vec<f32> {
    input.iter().map(|&s| processor.process_sample(s)).collect()
}

/// A unit impulse: `1.0` at index 0, silence after. Length `len`.
pub fn impulse(len: usize) -> Vec<f32> {
    let mut v = vec![0.0; len];
    if len > 0 {
        v[0] = 1.0;
    }
    v
}

/// `burst_len` samples of deterministic white-ish noise in `[-amp, amp]`,
/// followed by silence up to `total_len`.
pub fn noise_burst(total_len: usize, burst_len: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut v = vec![0.0; total_len];
    let mut state = seed | 1;
    for slot in v.iter_mut().take(burst_len.min(total_len)) {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = (state >> 40) as f32 / (1u64 << 24) as f32; // [0, 1)
        *slot = (unit * 2.0 - 1.0) * amp;
    }
    v
}

/// A pure sine of `freq` Hz at `amp`, length `len`.
pub fn sine(len: usize, freq: f32, sample_rate: f32, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| (i as f32 / sample_rate * freq * std::f32::consts::TAU).sin() * amp)
        .collect()
}

/// Local maxima of `|signal|` at or above `threshold`, de-duplicated so peaks
/// closer than `min_gap` samples collapse to the strongest. Returns
/// `(index, abs_amplitude)` in time order.
pub fn peaks(signal: &[f32], threshold: f32, min_gap: usize) -> Vec<(usize, f32)> {
    let mut found: Vec<(usize, f32)> = Vec::new();
    for i in 0..signal.len() {
        let a = signal[i].abs();
        if a < threshold {
            continue;
        }
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(signal.len() - 1);
        if a < signal[lo].abs() || a < signal[hi].abs() {
            continue;
        }
        match found.last_mut() {
            Some((pi, pa)) if i - *pi < min_gap => {
                if a > *pa {
                    *pi = i;
                    *pa = a;
                }
            }
            _ => found.push((i, a)),
        }
    }
    found
}

/// Spectral centroid (Hz) of `segment` — the magnitude-weighted mean frequency.
/// Higher = brighter. A Hann window suppresses edge leakage.
pub fn spectral_centroid(segment: &[f32], sample_rate: f32) -> f32 {
    let spectrum = magnitude_spectrum(segment);
    let n = (spectrum.len() - 1) * 2;
    let mut weighted = 0.0;
    let mut total = 0.0;
    for (k, &mag) in spectrum.iter().enumerate() {
        let freq = k as f32 * sample_rate / n as f32;
        weighted += freq * mag;
        total += mag;
    }
    if total > 0.0 {
        weighted / total
    } else {
        0.0
    }
}

/// Ratio of harmonic energy at `2·f0 + 3·f0` to the energy at `f0`. ~0 for a
/// linear path; rises with saturation. `signal` should be a steady sine at `f0`.
pub fn harmonic_ratio(signal: &[f32], f0: f32, sample_rate: f32) -> f32 {
    let spectrum = magnitude_spectrum(signal);
    let n = (spectrum.len() - 1) * 2;
    let bin = |freq: f32| ((freq * n as f32 / sample_rate).round() as usize).min(spectrum.len() - 1);
    let at = |freq: f32| {
        let b = bin(freq);
        // take the local max over ±1 bin to tolerate rounding/leakage
        let lo = b.saturating_sub(1);
        let hi = (b + 1).min(spectrum.len() - 1);
        spectrum[lo..=hi].iter().cloned().fold(0.0_f32, f32::max)
    };
    let fundamental = at(f0);
    if fundamental <= f32::EPSILON {
        return 0.0;
    }
    (at(2.0 * f0) + at(3.0 * f0)) / fundamental
}

/// Relative RMS difference between two equal-length renders: `0.0` = identical,
/// grows as they diverge. Used to prove one model is audibly distinct.
pub fn rms_difference(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "renders must be the same length");
    let mut diff_sq = 0.0;
    let mut ref_sq = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff_sq += (x - y) * (x - y);
        ref_sq += y * y;
    }
    if ref_sq > 0.0 {
        (diff_sq / ref_sq).sqrt()
    } else {
        diff_sq.sqrt()
    }
}

/// Hann-windowed magnitude spectrum, zero-padded to the next power of two.
fn magnitude_spectrum(segment: &[f32]) -> Vec<f32> {
    let n = segment.len().next_power_of_two().max(2);
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(n);
    let mut input = r2c.make_input_vec();
    let denom = (segment.len().max(2) - 1) as f32;
    for (i, &s) in segment.iter().enumerate() {
        let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / denom).cos();
        input[i] = s * w;
    }
    let mut spectrum = r2c.make_output_vec();
    r2c.process(&mut input, &mut spectrum).expect("fft");
    spectrum.iter().map(|c| c.norm()).collect()
}
