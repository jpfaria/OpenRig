//! Responsibility: reads distortion, aliasing and DC out of a rendered tone.

use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

use super::signals::FFT_SIZE;

/// Half-width, in bins, of the 4-term Blackman-Harris main lobe.
const SKIRT: usize = 4;

/// Power per positive-frequency bin of the last `FFT_SIZE` samples,
/// Blackman-Harris windowed (sidelobes at -92 dB).
pub fn power_spectrum(signal: &[f32]) -> Vec<f64> {
    assert!(signal.len() >= FFT_SIZE, "render shorter than the FFT");
    let tail = &signal[signal.len() - FFT_SIZE..];
    let n = FFT_SIZE as f64;
    let mut buf: Vec<Complex<f64>> = tail
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let x = std::f64::consts::TAU * i as f64 / n;
            let w =
                0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos();
            Complex::new(s as f64 * w, 0.0)
        })
        .collect();
    FftPlanner::<f64>::new()
        .plan_fft_forward(FFT_SIZE)
        .process(&mut buf);
    buf.iter()
        .take(FFT_SIZE / 2)
        .map(|c| c.norm_sqr())
        .collect()
}

fn lobe(spectrum: &[f64], bin: usize) -> f64 {
    let lo = bin.saturating_sub(SKIRT);
    let hi = (bin + SKIRT).min(spectrum.len() - 1);
    spectrum[lo..=hi].iter().sum()
}

fn ratio_db(part: f64, whole: f64) -> f32 {
    if whole <= 0.0 {
        return 0.0;
    }
    (10.0 * (part.max(1e-30) / whole).log10()) as f32
}

/// Everything except DC and the fundamental, relative to the fundamental,
/// dB. -60 dB is 0.1 % THD+N.
pub fn thd_n_db(spectrum: &[f64], fundamental_bin: usize) -> f32 {
    let fundamental = lobe(spectrum, fundamental_bin);
    let total: f64 = spectrum[SKIRT + 1..].iter().sum();
    ratio_db(total - fundamental, fundamental)
}

/// Energy below `top_bin` that is neither DC nor a harmonic of the
/// fundamental, relative to the fundamental, dB. For a tone whose bin does
/// not divide the FFT size, folded (aliased) harmonics land here, together
/// with noise.
pub fn inharmonic_db(spectrum: &[f64], fundamental_bin: usize, top_bin: usize) -> f32 {
    let half = spectrum.len().min(top_bin);
    let mut harmonic = vec![false; half];
    harmonic[..=SKIRT].iter_mut().for_each(|h| *h = true);
    // A harmonic just above `top_bin` still has skirt bins below it.
    let mut n = 1;
    while n * fundamental_bin < half + SKIRT {
        let centre = n * fundamental_bin;
        let lo = centre.saturating_sub(SKIRT);
        let hi = (centre + SKIRT).min(half - 1);
        if lo <= hi {
            harmonic[lo..=hi].iter_mut().for_each(|h| *h = true);
        }
        n += 1;
    }
    let rest: f64 = spectrum[..half]
        .iter()
        .zip(&harmonic)
        .filter(|(_, &h)| !h)
        .map(|(p, _)| p)
        .sum();
    ratio_db(rest, lobe(spectrum, fundamental_bin))
}

/// Mean of the last `FFT_SIZE` samples, dBFS.
pub fn dc_dbfs(signal: &[f32]) -> f32 {
    let tail = &signal[signal.len().saturating_sub(FFT_SIZE)..];
    let mean = tail.iter().map(|&s| s as f64).sum::<f64>() / tail.len().max(1) as f64;
    lin_db(mean.abs() as f32)
}

/// RMS of the last `len` samples, dBFS.
pub fn tail_rms_dbfs(signal: &[f32], len: usize) -> f32 {
    let tail = &signal[signal.len().saturating_sub(len)..];
    let sum: f64 = tail.iter().map(|&s| (s as f64) * (s as f64)).sum();
    lin_db((sum / tail.len().max(1) as f64).sqrt() as f32)
}

/// Peak of a whole render, dBFS.
pub fn peak_dbfs(signal: &[f32]) -> f32 {
    lin_db(signal.iter().fold(0.0_f32, |m, &s| m.max(s.abs())))
}

pub fn lin_db(x: f32) -> f32 {
    if !x.is_finite() {
        return f32::INFINITY;
    }
    if x <= 1e-10 {
        return -200.0;
    }
    20.0 * x.log10()
}
