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

const FFT_SIZE: usize = 2048;
const N_BANDS: usize = 16;

// Center frequencies for 16 logarithmic bands (Hz)
const BAND_FREQS: [f32; N_BANDS] = [
    25.0, 40.0, 63.0, 100.0, 160.0, 250.0, 400.0, 630.0, 1000.0, 1600.0, 2500.0, 4000.0,
    6300.0, 10000.0, 16000.0, 20000.0,
];

// Display labels for each band
const BAND_LABELS: [&str; N_BANDS] = [
    "25", "40", "63", "100", "160", "250", "400", "630", "1k", "1.6k", "2.5k", "4k", "6.3k",
    "10k", "16k", "20k",
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
        // dt = time per FFT frame; tau = 0.5s decay time constant
        let dt = FFT_SIZE as f32 / sample_rate;
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
        let dt = FFT_SIZE as f32 / sample_rate;
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
    buffer: Vec<f32>,
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
            buffer: Vec::with_capacity(FFT_SIZE),
            tx,
        }
    }
}

impl MonoProcessor for SpectrumAnalyzer {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.buffer.push(input);
        if self.buffer.len() >= FFT_SIZE {
            let buf = std::mem::replace(&mut self.buffer, Vec::with_capacity(FFT_SIZE));
            // Non-blocking send: drop frame if worker is busy
            let _ = self.tx.try_send(buf);
        }
        input // pass through unchanged
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
