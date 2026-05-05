//! 63-band 1/6-octave spectrum analyzer DSP.
//!
//! Pure, self-contained DSP. The caller owns sample accumulation, threading
//! and any UI publishing — this module just runs the FFT and produces a
//! snapshot of band levels (with attack/decay smoothing) and peak holds.
//!
//! - FFT size 8192, Hann window, 75% overlap (HOP_SIZE = 2048)
//! - 63 logarithmic bands centered at `f[i] = 20 * 2^(i/6)` (1/6-octave)
//! - Range 20 Hz – ~25.8 kHz
//! - Levels normalized 0.0..1.0 (corresponds to -80 dBFS..0 dBFS)
//! - Smoother: instant attack, exponential decay (τ ≈ 0.5 s)
//! - Peak hold: instant attack, exponential decay (τ ≈ 2.5 s)
//!
//! ```rust,no_run
//! use feature_dsp::spectrum_fft::{SpectrumAnalyzer, FFT_SIZE};
//! let mut analyzer = SpectrumAnalyzer::new(48_000.0);
//! let buffer = vec![0.0_f32; FFT_SIZE];
//! let snapshot = analyzer.process(&buffer);
//! assert_eq!(snapshot.levels.len(), feature_dsp::spectrum_fft::N_BANDS);
//! ```

use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;

/// FFT size in samples.
pub const FFT_SIZE: usize = 8192;
/// Hop size in samples (75 % overlap → ~23 Hz refresh @ 48 kHz).
pub const HOP_SIZE: usize = 2048;
/// Number of logarithmic bands.
pub const N_BANDS: usize = 63;

/// Center frequencies for the 63 logarithmic bands (Hz). 1/6-octave spacing.
pub const BAND_FREQS: [f32; N_BANDS] = [
    20.0, 22.4, 25.2, 28.3, 31.7, 35.6, // i=0..5   20–36 Hz
    40.0, 44.9, 50.4, 56.6, 63.5, 71.3, // i=6..11  40–71 Hz
    80.0, 89.8, 100.8, 113.1, 127.0, 142.5, // i=12..17 80–143 Hz
    160.0, 179.6, 201.6, 226.3, 254.0, 285.1, // i=18..23 160–285 Hz
    320.0, 359.2, 403.2, 452.5, 508.0, 570.2, // i=24..29 320–570 Hz
    640.0, 718.4, 806.3, 905.1, 1016.0, 1140.4, // i=30..35 640 Hz–1.14 kHz
    1280.0, 1436.8, 1612.7, 1810.2, 2031.9, 2280.7, // i=36..41 1.28–2.28 kHz
    2560.0, 2873.5, 3225.4, 3620.4, 4063.7, 4561.4, // i=42..47 2.56–4.56 kHz
    5120.0, 5747.0, 6450.8, 7240.8, 8127.5, 9122.8, // i=48..53 5.12–9.12 kHz
    10240.0, 11494.0, 12901.6, 14481.5, 16255.0, 18245.6, // i=54..59 10.24–18.25 kHz
    20480.0, 22988.0, 25803.2, // i=60..62 20.5–25.8 kHz
];

/// Display labels for the 63 bands (sparse — most bands are unlabeled to
/// avoid clutter at narrow bar widths).
pub const BAND_LABELS: [&str; N_BANDS] = [
    "20", "", "", "", "", "", // i=0..5
    "", "", "50", "", "", "", // i=6..11
    "", "", "100", "", "", "", // i=12..17
    "", "", "200", "", "", "", // i=18..23
    "", "", "", "", "500", "", // i=24..29
    "", "", "", "", "1k", "", // i=30..35
    "", "", "", "", "2k", "", // i=36..41
    "", "", "", "", "", "", // i=42..47
    "5k", "", "", "", "", "", // i=48..53
    "10k", "", "", "", "", "", // i=54..59
    "20k", "", "", // i=60..62
];

/// Smoother decay time constant (s).
const SMOOTHER_TAU: f32 = 0.5;
/// Peak hold decay time constant (s).
const PEAK_TAU: f32 = 2.5;

/// FFT bin index for a given frequency, clamped to the Nyquist range.
fn freq_to_bin(freq: f32, sample_rate: f32) -> usize {
    ((freq * FFT_SIZE as f32 / sample_rate) as usize).min(FFT_SIZE / 2 - 1)
}

/// `[low_bin, high_bin)` range for band `i` using geometric midpoints.
fn band_bin_range(i: usize, sample_rate: f32) -> (usize, usize) {
    let low_freq = if i == 0 {
        0.0
    } else {
        (BAND_FREQS[i - 1] * BAND_FREQS[i]).sqrt()
    };
    let high_freq = if i == N_BANDS - 1 {
        sample_rate / 2.0
    } else {
        (BAND_FREQS[i] * BAND_FREQS[i + 1]).sqrt()
    };
    let lo = freq_to_bin(low_freq, sample_rate);
    let hi = freq_to_bin(high_freq, sample_rate).max(lo + 1);
    (lo, hi)
}

/// Per-band level with instant attack + exponential decay.
struct BandSmoother {
    levels: [f32; N_BANDS],
    decay_factor: f32,
}

impl BandSmoother {
    fn new(sample_rate: f32) -> Self {
        let dt = HOP_SIZE as f32 / sample_rate;
        Self {
            levels: [0.0; N_BANDS],
            decay_factor: (-dt / SMOOTHER_TAU).exp(),
        }
    }

    fn update(&mut self, new_levels: &[f32; N_BANDS]) {
        for i in 0..N_BANDS {
            if new_levels[i] > self.levels[i] {
                self.levels[i] = new_levels[i];
            } else {
                self.levels[i] *= self.decay_factor;
            }
        }
    }
}

/// Per-band peak hold: jumps instantly to new peak, then decays exponentially.
struct PeakHolder {
    peaks: [f32; N_BANDS],
    decay_factor: f32,
}

impl PeakHolder {
    fn new(sample_rate: f32) -> Self {
        let dt = HOP_SIZE as f32 / sample_rate;
        Self {
            peaks: [0.0; N_BANDS],
            decay_factor: (-dt / PEAK_TAU).exp(),
        }
    }

    fn update(&mut self, levels: &[f32; N_BANDS]) {
        for i in 0..N_BANDS {
            if levels[i] >= self.peaks[i] {
                self.peaks[i] = levels[i];
            } else {
                self.peaks[i] *= self.decay_factor;
            }
        }
    }
}

/// One snapshot of band levels + peak holds, normalized 0.0..1.0.
#[derive(Debug, Clone)]
pub struct SpectrumSnapshot {
    pub levels: [f32; N_BANDS],
    pub peaks: [f32; N_BANDS],
}

/// 63-band spectrum analyzer with built-in 75 % overlap sliding window.
///
/// Feed any number of samples through [`SpectrumAnalyzer::process_chunk`];
/// the analyzer keeps a `FFT_SIZE`-sized circular history and returns
/// `Some(snapshot)` every time `HOP_SIZE` samples have accumulated since
/// the previous FFT (~23 Hz refresh @ 48 kHz). The snapshot reflects the
/// **most recent** FFT in the chunk — if a chunk crosses several HOP_SIZE
/// boundaries (large UI tick), only the last frame is materialised.
///
/// The legacy [`SpectrumAnalyzer::process`] one-shot API is preserved for
/// tests and any caller that already hands over an `FFT_SIZE`-sized buffer.
///
/// The analyzer is sample-rate aware: bin ranges and smoother decay rates
/// are pre-computed once at construction. To re-use across rates, build a
/// new instance.
pub struct SpectrumAnalyzer {
    fft: Arc<dyn Fft<f32>>,
    hann: Vec<f32>,
    bin_ranges: [(usize, usize); N_BANDS],
    smoother: BandSmoother,
    peaks: PeakHolder,
    /// Reusable scratch buffer to avoid per-FFT allocation.
    scratch: Vec<Complex<f32>>,
    /// Circular history of the last `FFT_SIZE` samples. Filled lazily by
    /// `process_chunk`; the sliding window reads from this to avoid the
    /// caller having to assemble a contiguous buffer.
    history: Vec<f32>,
    /// Next write position in `history` (wraps mod `FFT_SIZE`).
    history_write: usize,
    /// Sample counter since the last FFT dispatch.
    hop_count: usize,
}

impl SpectrumAnalyzer {
    pub fn new(sample_rate: f32) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
            })
            .collect();

        let mut bin_ranges = [(0usize, 1usize); N_BANDS];
        for (i, r) in bin_ranges.iter_mut().enumerate() {
            *r = band_bin_range(i, sample_rate);
        }

        Self {
            fft,
            hann,
            bin_ranges,
            smoother: BandSmoother::new(sample_rate),
            peaks: PeakHolder::new(sample_rate),
            scratch: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            history: vec![0.0; FFT_SIZE],
            history_write: 0,
            hop_count: 0,
        }
    }

    pub const fn buffer_size() -> usize {
        FFT_SIZE
    }

    /// Run one FFT pass over the current window (read in chronological
    /// order from `history_write`) and return the updated band snapshot.
    fn run_fft(&mut self) -> SpectrumSnapshot {
        let start = self.history_write;
        for i in 0..FFT_SIZE {
            let s = self.history[(start + i) % FFT_SIZE];
            self.scratch[i] = Complex::new(s * self.hann[i], 0.0);
        }
        self.fft.process(&mut self.scratch);

        let scale = 2.0 / FFT_SIZE as f32;
        let mut new_levels = [0.0_f32; N_BANDS];
        for (i, &(lo, hi)) in self.bin_ranges.iter().enumerate() {
            let mag = self.scratch[lo..hi.min(FFT_SIZE / 2)]
                .iter()
                .map(|c| c.norm() * scale)
                .fold(0.0_f32, f32::max);
            let db = 20.0 * mag.max(1e-10).log10();
            new_levels[i] = ((db + 80.0) / 80.0).clamp(0.0, 1.0);
        }

        self.smoother.update(&new_levels);
        self.peaks.update(&self.smoother.levels);

        SpectrumSnapshot {
            levels: self.smoother.levels,
            peaks: self.peaks.peaks,
        }
    }

    /// Push samples through the sliding window. Returns the latest snapshot
    /// if any `HOP_SIZE` boundary was crossed during this chunk; `None`
    /// otherwise. Allocation-free (history + scratch are pre-allocated).
    pub fn process_chunk(&mut self, samples: &[f32]) -> Option<SpectrumSnapshot> {
        let mut latest: Option<SpectrumSnapshot> = None;
        for &s in samples {
            self.history[self.history_write] = s;
            self.history_write = (self.history_write + 1) % FFT_SIZE;
            self.hop_count += 1;
            if self.hop_count >= HOP_SIZE {
                self.hop_count = 0;
                latest = Some(self.run_fft());
            }
        }
        latest
    }

    /// Run one FFT pass over an externally supplied buffer of exactly
    /// [`FFT_SIZE`] samples. Kept for tests and callers that pre-assemble
    /// the window themselves; production paths should prefer
    /// [`Self::process_chunk`] which manages the sliding history and
    /// 75 % overlap automatically.
    pub fn process(&mut self, buffer: &[f32]) -> SpectrumSnapshot {
        for i in 0..FFT_SIZE {
            let s = buffer.get(i).copied().unwrap_or(0.0);
            self.scratch[i] = Complex::new(s * self.hann[i], 0.0);
        }
        self.fft.process(&mut self.scratch);

        let scale = 2.0 / FFT_SIZE as f32;
        let mut new_levels = [0.0_f32; N_BANDS];
        for (i, &(lo, hi)) in self.bin_ranges.iter().enumerate() {
            let mag = self.scratch[lo..hi.min(FFT_SIZE / 2)]
                .iter()
                .map(|c| c.norm() * scale)
                .fold(0.0_f32, f32::max);
            let db = 20.0 * mag.max(1e-10).log10();
            new_levels[i] = ((db + 80.0) / 80.0).clamp(0.0, 1.0);
        }

        self.smoother.update(&new_levels);
        self.peaks.update(&self.smoother.levels);

        SpectrumSnapshot {
            levels: self.smoother.levels,
            peaks: self.peaks.peaks,
        }
    }
}

#[cfg(test)]
#[path = "spectrum_fft_tests.rs"]
mod tests;
