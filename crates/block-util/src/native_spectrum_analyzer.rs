use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StreamEntry, StreamHandle,
};
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{Arc, Mutex};

use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;

pub const MODEL_ID: &str = "spectrum_analyzer";
pub const DISPLAY_NAME: &str = "Spectrum Analyzer";

const FFT_SIZE: usize = 8192;
// 75% overlap → update rate = sample_rate / HOP_SIZE ≈ 23 Hz at 48 kHz (fluid)
const HOP_SIZE: usize = 2048;
const N_BANDS: usize = 63;

// Center frequencies for 63 logarithmic bands (Hz): f[i] = 20 * 2^(i/6), 1/6-octave spacing.
// Range: 20 Hz – ~25.8 kHz
const BAND_FREQS: [f32; N_BANDS] = [
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

// Labels shown in the UI — only key frequencies to avoid clutter at narrow bar widths.
// Labeled bands (approx): 20, 50, 100, 200, 500, 1k, 2k, 5k, 10k, 20k Hz.
const BAND_LABELS: [&str; N_BANDS] = [
    "20", "", "", "", "", "",        // i=0..5
    "", "", "50", "", "", "",        // i=6..11
    "", "", "100", "", "", "",       // i=12..17
    "", "", "200", "", "", "",       // i=18..23
    "", "", "", "", "500", "",       // i=24..29
    "", "", "", "", "1k", "",        // i=30..35
    "", "", "", "", "2k", "",        // i=36..41
    "", "", "", "", "", "",          // i=42..47
    "5k", "", "", "", "", "",        // i=48..53
    "10k", "", "", "", "", "",       // i=54..59
    "20k", "", "",                   // i=60..62
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "utility".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![],
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

/// Computes FFT bin index for a given frequency.
fn freq_to_bin(freq: f32, sample_rate: f32) -> usize {
    ((freq * FFT_SIZE as f32 / sample_rate) as usize).min(FFT_SIZE / 2 - 1)
}

/// Returns the [low_bin, high_bin) range for band i using geometric midpoints.
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

/// Per-band level with attack/decay smoothing.
struct BandSmoother {
    levels: [f32; N_BANDS],
    decay_factor: f32,
}

impl BandSmoother {
    fn new(sample_rate: f32) -> Self {
        // dt = time per hop (update rate); tau = 0.5s decay time constant
        let dt = HOP_SIZE as f32 / sample_rate;
        let decay_factor = (-dt / 0.5_f32).exp();
        Self {
            levels: [0.0; N_BANDS],
            decay_factor,
        }
    }

    /// Update levels with new instantaneous magnitudes (0.0-1.0 normalized).
    fn update(&mut self, new_levels: &[f32; N_BANDS]) {
        for i in 0..N_BANDS {
            if new_levels[i] > self.levels[i] {
                // Instantaneous attack
                self.levels[i] = new_levels[i];
            } else {
                // Exponential decay
                self.levels[i] *= self.decay_factor;
            }
        }
    }
}

// Peak hold: decays from peak to 0 over ~2.5s
const PEAK_DECAY_TAU: f32 = 2.5;

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
            decay_factor: (-dt / PEAK_DECAY_TAU).exp(),
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

struct SpectrumWorker {
    fft: Arc<dyn rustfft::Fft<f32>>,
    hann: Vec<f32>,
    bin_ranges: [(usize, usize); N_BANDS],
    smoother: BandSmoother,
    peaks: PeakHolder,
    stream: StreamHandle,
}

impl SpectrumWorker {
    fn new(sample_rate: f32, stream: StreamHandle) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        // Hann window coefficients
        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
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
            stream,
        }
    }

    fn process(&mut self, buffer: &[f32]) {
        // Apply Hann window and convert to complex
        let mut input: Vec<Complex<f32>> = buffer
            .iter()
            .zip(self.hann.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();

        self.fft.process(&mut input);

        // Compute magnitude for each band (max bin in range, normalized to 0.0-1.0)
        let scale = 2.0 / FFT_SIZE as f32;
        let mut new_levels = [0.0f32; N_BANDS];
        for (i, &(lo, hi)) in self.bin_ranges.iter().enumerate() {
            let mag = input[lo..hi.min(FFT_SIZE / 2)]
                .iter()
                .map(|c| c.norm() * scale)
                .fold(0.0f32, f32::max);

            // Convert to dB then normalize: -80dBFS..0dBFS → 0.0..1.0
            let db = 20.0 * mag.max(1e-10).log10();
            new_levels[i] = ((db + 80.0) / 80.0).clamp(0.0, 1.0);
        }

        self.smoother.update(&new_levels);
        self.peaks.update(&self.smoother.levels);

        // Write to stream handle
        if let Ok(mut entries) = self.stream.lock() {
            entries.clear();
            for (i, &level) in self.smoother.levels.iter().enumerate() {
                entries.push(StreamEntry {
                    key: format!("band_{i}"),
                    value: level,
                    text: BAND_LABELS[i].to_string(),
                    peak: self.peaks.peaks[i],
                });
            }
        }
    }
}

pub struct SpectrumAnalyzer {
    // Ring buffer always holds the last FFT_SIZE samples
    ring: Vec<f32>,
    write_pos: usize,
    // Counts new samples since last FFT dispatch
    hop_count: usize,
    // Pre-allocated send buffer — avoids heap allocation in the RT thread
    buf: Vec<f32>,
    tx: std::sync::mpsc::SyncSender<Vec<f32>>,
}

impl SpectrumAnalyzer {
    pub fn new(sample_rate: f32, stream: StreamHandle) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(1);
        let mut worker = SpectrumWorker::new(sample_rate, stream);

        std::thread::Builder::new()
            .name("spectrum-analyzer".to_string())
            .spawn(move || {
                for buf in rx {
                    worker.process(&buf);
                }
            })
            .expect("spawn spectrum worker");

        Self {
            ring: vec![0.0; FFT_SIZE],
            write_pos: 0,
            hop_count: 0,
            buf: vec![0.0; FFT_SIZE],
            tx,
        }
    }
}

impl MonoProcessor for SpectrumAnalyzer {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.ring[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % FFT_SIZE;
        self.hop_count += 1;

        if self.hop_count >= HOP_SIZE {
            self.hop_count = 0;
            // Copy ring buffer in chronological order (oldest sample first)
            let read_start = self.write_pos;
            for i in 0..FFT_SIZE {
                self.buf[i] = self.ring[(read_start + i) % FFT_SIZE];
            }
            // Non-blocking: drop frame if worker is still busy with previous
            let _ = self.tx.try_send(self.buf.clone());
        }
        input
    }
}

fn build(
    _params: &ParameterSet,
    sample_rate: usize,
    layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    match layout {
        AudioChannelLayout::Mono => {
            let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
            let processor = SpectrumAnalyzer::new(sample_rate as f32, Arc::clone(&stream));
            Ok((BlockProcessor::Mono(Box::new(processor)), Some(stream)))
        }
        AudioChannelLayout::Stereo => anyhow::bail!(
            "spectrum_analyzer uses DualMono; engine should never call build with Stereo layout"
        ),
    }
}

pub const MODEL_DEFINITION: UtilModelDefinition = UtilModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: UtilBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
    stream_kind: "spectrum",
};
