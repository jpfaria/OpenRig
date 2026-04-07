use anyhow::{Error, Result};
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StreamEntry, StreamHandle,
};
use std::sync::{Arc, Mutex};

use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;

pub const MODEL_ID: &str = "tuner_chromatic";
pub const DISPLAY_NAME: &str = "Chromatic Tuner";

// --- YIN constants ---
// 2048 samples ≈ 42ms at 48kHz; enough for ~2.3 periods of 55Hz (lowest string).
const BUFFER_SIZE: usize = 2048;
const MIN_DETECTION: usize = 1024;
const MIN_FREQ: f32 = 55.0; // A1
const MAX_FREQ: f32 = 1200.0;
const RMS_SILENCE_THRESHOLD: f32 = 0.005;
const YIN_ABSOLUTE_THRESHOLD: f32 = 0.15;
const YIN_REJECT_THRESHOLD: f32 = 0.4;

// --- Smoothing / debounce ---
// α=0.25 → 90% convergence in ~8 windows (≈336ms at 48kHz). Fast enough for live tuning.
const EMA_ALPHA: f32 = 0.25;
const SNAP_RATIO: f32 = 1.06; // one semitone — snap immediately on large jumps
// 2 consecutive same-note detections before displaying (≈84ms at 48kHz).
const DEBOUNCE_COUNT: u32 = 2;

// --- Silence timeout (samples with no detection before clearing) ---
// At 48kHz, 48000 samples ≈ 1 second of sustained silence before clearing the display.
const SILENCE_TIMEOUT_SAMPLES: usize = 48_000;

const DEFAULT_REFERENCE_HZ: f32 = 440.0;

const NOTES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "utility".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "reference_hz",
                "Reference",
                None,
                Some(DEFAULT_REFERENCE_HZ),
                400.0,
                480.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            bool_parameter("mute_signal", "Mute Signal", None, Some(true)),
        ],
    }
}

/// Frequency to note name, octave, and cents offset.
fn freq_to_note(frequency: f32, reference_hz: f32) -> (&'static str, i32, f32) {
    let semitones_from_a4 = 12.0 * (frequency / reference_hz).log2();
    let note_number = semitones_from_a4.round() as i32;
    let cents = (semitones_from_a4 - note_number as f32) * 100.0;
    let note_index = note_number.rem_euclid(12) as usize;
    // div_euclid gives floor division for negative numbers (e.g. A2=-24: (-15).div_euclid(12)=-2)
    let octave = 4 + (note_number + 9).div_euclid(12);
    (NOTES[note_index], octave, cents)
}

pub struct ChromaticTuner {
    buffer: Vec<f32>,
    sample_rate: usize,
    reference_hz: f32,
    mute_signal: bool,
    stream: StreamHandle,

    // EMA-smoothed frequency
    smoothed_freq: Option<f32>,

    // Note debounce
    current_note: Option<String>,
    pending_note: Option<String>,
    pending_count: u32,

    // Silence timeout
    silent_samples: usize,
}

impl ChromaticTuner {
    pub fn new(sample_rate: usize, reference_hz: f32, mute_signal: bool, stream: StreamHandle) -> Self {
        Self {
            buffer: Vec::with_capacity(BUFFER_SIZE),
            sample_rate,
            reference_hz,
            mute_signal,
            stream,
            smoothed_freq: None,
            current_note: None,
            pending_note: None,
            pending_count: 0,
            silent_samples: 0,
        }
    }

    /// YIN difference function: d(tau) = sum of (x[i] - x[i+tau])^2
    fn yin_difference(buf: &[f32], max_tau: usize) -> Vec<f32> {
        let n = buf.len();
        let mut d = vec![0.0_f32; max_tau];
        for tau in 1..max_tau {
            let mut sum = 0.0;
            for i in 0..(n - tau) {
                let diff = buf[i] - buf[i + tau];
                sum += diff * diff;
            }
            d[tau] = sum;
        }
        d
    }

    /// Cumulative mean normalized difference: d'(tau)
    fn yin_cmnd(d: &[f32]) -> Vec<f32> {
        let mut d_prime = vec![0.0_f32; d.len()];
        d_prime[0] = 1.0;
        let mut running_sum = 0.0;
        for tau in 1..d.len() {
            running_sum += d[tau];
            if running_sum > 0.0 {
                d_prime[tau] = d[tau] * tau as f32 / running_sum;
            } else {
                d_prime[tau] = 1.0;
            }
        }
        d_prime
    }

    /// Find the best tau using absolute threshold on d'(tau).
    fn yin_find_tau(d_prime: &[f32], min_tau: usize, max_tau: usize) -> Option<(usize, f32)> {
        let upper = max_tau.min(d_prime.len());
        // Search for first tau below absolute threshold
        let mut tau = min_tau;
        while tau < upper {
            if d_prime[tau] < YIN_ABSOLUTE_THRESHOLD {
                // Walk past the dip to find the local minimum
                while tau + 1 < upper && d_prime[tau + 1] < d_prime[tau] {
                    tau += 1;
                }
                return Some((tau, d_prime[tau]));
            }
            tau += 1;
        }

        // Fallback: find global minimum below reject threshold
        let mut best_tau = None;
        let mut best_val = f32::MAX;
        for tau in min_tau..upper {
            if d_prime[tau] < best_val {
                best_val = d_prime[tau];
                best_tau = Some(tau);
            }
        }

        if best_val < YIN_REJECT_THRESHOLD {
            best_tau.map(|t| (t, best_val))
        } else {
            None
        }
    }

    /// Parabolic interpolation for sub-sample accuracy.
    fn parabolic_interpolation(d_prime: &[f32], tau: usize) -> f32 {
        if tau < 1 || tau + 1 >= d_prime.len() {
            return tau as f32;
        }
        let s0 = d_prime[tau - 1];
        let s1 = d_prime[tau];
        let s2 = d_prime[tau + 1];
        let denom = 2.0 * s1 - s2 - s0;
        if denom.abs() < 1e-12 {
            tau as f32
        } else {
            tau as f32 + (s2 - s0) / (2.0 * denom)
        }
    }

    /// Octave validation: check sub-harmonic at tau*2.
    fn octave_check(d_prime: &[f32], best_tau: usize, sample_rate: usize) -> usize {
        let sub_tau = best_tau * 2;
        if sub_tau >= d_prime.len() {
            return best_tau;
        }
        let sub_freq = sample_rate as f32 / sub_tau as f32;
        if sub_freq < MIN_FREQ {
            return best_tau;
        }
        // If sub-harmonic has a reasonably strong dip, prefer it (lower octave)
        if d_prime[sub_tau] < d_prime[best_tau] * 1.5 {
            sub_tau
        } else {
            best_tau
        }
    }

    /// Run YIN pitch detection on the current buffer.
    fn detect_pitch(&self) -> Option<f32> {
        let n = self.buffer.len();
        if n < MIN_DETECTION {
            return None;
        }

        // RMS silence check
        let rms = (self.buffer.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
        if rms < RMS_SILENCE_THRESHOLD {
            return None;
        }

        let min_tau = (self.sample_rate as f32 / MAX_FREQ).ceil() as usize;
        let max_tau = (self.sample_rate as f32 / MIN_FREQ).floor() as usize;
        let max_tau = max_tau.min(n / 2);

        if min_tau >= max_tau {
            return None;
        }

        let d = Self::yin_difference(&self.buffer, max_tau + 1);
        let d_prime = Self::yin_cmnd(&d);

        let (best_tau, _best_val) = Self::yin_find_tau(&d_prime, min_tau, max_tau)?;

        // Octave validation
        let checked_tau = Self::octave_check(&d_prime, best_tau, self.sample_rate);

        // Parabolic interpolation
        let refined_tau = Self::parabolic_interpolation(&d_prime, checked_tau);

        if refined_tau <= 0.0 {
            return None;
        }

        let freq = self.sample_rate as f32 / refined_tau;
        if freq < MIN_FREQ || freq > MAX_FREQ {
            return None;
        }

        Some(freq)
    }

    /// Apply EMA smoothing. Snap to new value if jump exceeds one semitone.
    fn smooth_frequency(&mut self, raw_freq: f32) -> f32 {
        match self.smoothed_freq {
            None => {
                self.smoothed_freq = Some(raw_freq);
                raw_freq
            }
            Some(prev) => {
                let ratio = raw_freq / prev;
                if ratio > SNAP_RATIO || ratio < 1.0 / SNAP_RATIO {
                    // Large jump — snap immediately
                    self.smoothed_freq = Some(raw_freq);
                    raw_freq
                } else {
                    let smoothed = prev * (1.0 - EMA_ALPHA) + raw_freq * EMA_ALPHA;
                    self.smoothed_freq = Some(smoothed);
                    smoothed
                }
            }
        }
    }

    /// Apply note debounce: only update displayed note after N consecutive same readings.
    fn debounce_note(&mut self, note_str: &str) {
        if self.pending_note.as_deref() == Some(note_str) {
            self.pending_count += 1;
        } else {
            self.pending_note = Some(note_str.to_string());
            self.pending_count = 1;
        }
        if self.pending_count >= DEBOUNCE_COUNT {
            self.current_note = Some(note_str.to_string());
        }
    }

    /// Write current reading to the stream handle.
    fn publish_stream(&self, frequency: f32, note: &str, cents: f32) {
        if let Ok(mut entries) = self.stream.lock() {
            entries.clear();
            entries.push(StreamEntry {
                key: "note".to_string(),
                value: 0.0,
                text: note.to_string(),
            });
            entries.push(StreamEntry {
                key: "cents".to_string(),
                value: cents,
                text: format!("{cents:+.1}"),
            });
            entries.push(StreamEntry {
                key: "frequency".to_string(),
                value: frequency,
                text: format!("{frequency:.1} Hz"),
            });
        }
    }

    fn clear_stream(&self) {
        if let Ok(mut entries) = self.stream.lock() {
            entries.clear();
        }
    }

    /// Called after accumulating enough samples; runs detection + smoothing + debounce.
    fn run_detection(&mut self) {
        match self.detect_pitch() {
            Some(raw_freq) => {
                self.silent_samples = 0;
                let freq = self.smooth_frequency(raw_freq);
                let (note_name, _octave, cents) = freq_to_note(freq, self.reference_hz);
                // Debounce on note name only — octave estimation can fluctuate for the
                // same string, which would cause the debounce to reset constantly.
                self.debounce_note(note_name);

                if let Some(ref current) = self.current_note {
                    self.publish_stream(freq, current, cents);
                }
            }
            None => {
                self.silent_samples += self.buffer.len();
                if self.silent_samples >= SILENCE_TIMEOUT_SAMPLES {
                    self.smoothed_freq = None;
                    self.current_note = None;
                    self.pending_note = None;
                    self.pending_count = 0;
                    self.clear_stream();
                }
            }
        }
        self.buffer.clear();
    }
}

impl MonoProcessor for ChromaticTuner {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.buffer.push(input);

        // Run detection when buffer is full
        if self.buffer.len() >= BUFFER_SIZE {
            self.run_detection();
        }

        if self.mute_signal {
            0.0
        } else {
            input
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: usize,
    _layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    let reference_hz = required_f32(params, "reference_hz").map_err(Error::msg)?;
    let mute_signal = required_bool(params, "mute_signal").map_err(Error::msg)?;

    let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
    let tuner = ChromaticTuner::new(sample_rate, reference_hz, mute_signal, Arc::clone(&stream));
    let processor = BlockProcessor::Mono(Box::new(tuner));

    Ok((processor, Some(stream)))
}

pub const MODEL_DEFINITION: UtilModelDefinition = UtilModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: UtilBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: generate a sine wave at the given frequency.
    fn sine_wave(freq: f32, sample_rate: usize, num_samples: usize) -> Vec<f32> {
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect()
    }

    /// Helper: create a tuner with default settings and feed samples, returning stream entries.
    fn run_tuner(
        freq: f32,
        sample_rate: usize,
        reference_hz: f32,
        mute: bool,
    ) -> (Vec<StreamEntry>, Vec<f32>) {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(sample_rate, reference_hz, mute, Arc::clone(&stream));

        // Generate enough samples for reliable detection (multiple buffers)
        let samples = sine_wave(freq, sample_rate, BUFFER_SIZE * 3);
        let mut outputs = Vec::with_capacity(samples.len());
        for s in &samples {
            outputs.push(tuner.process_sample(*s));
        }

        let entries = stream.lock().unwrap().clone();
        (entries, outputs)
    }

    fn find_entry<'a>(entries: &'a [StreamEntry], key: &str) -> Option<&'a StreamEntry> {
        entries.iter().find(|e| e.key == key)
    }

    #[test]
    fn detect_a4_440hz() {
        let (entries, _) = run_tuner(440.0, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "A", "Expected A, got {}", note.text);
    }

    #[test]
    fn detect_e2_82hz() {
        let (entries, _) = run_tuner(82.41, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "E", "Expected E, got {}", note.text);
    }

    #[test]
    fn detect_high_e4_330hz() {
        let (entries, _) = run_tuner(329.63, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "E", "Expected E, got {}", note.text);
    }

    #[test]
    fn silence_produces_no_reading() {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(44100, 440.0, true, Arc::clone(&stream));

        // Feed silence (zeros)
        for _ in 0..(BUFFER_SIZE * 3) {
            tuner.process_sample(0.0);
        }

        let entries = stream.lock().unwrap();
        assert!(entries.is_empty(), "Silence should produce no stream entries");
    }

    #[test]
    fn mute_zeroes_output() {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(44100, 440.0, true, Arc::clone(&stream));
        let output = tuner.process_sample(0.5);
        assert_eq!(output, 0.0, "Muted tuner should output 0.0");
    }

    #[test]
    fn passthrough_preserves_signal() {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(44100, 440.0, false, Arc::clone(&stream));
        let output = tuner.process_sample(0.5);
        assert_eq!(output, 0.5, "Passthrough tuner should preserve signal");
    }

    #[test]
    fn reference_hz_shifts_detection() {
        let (entries, _) = run_tuner(432.0, 44100, 432.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "A", "432Hz with 432Hz ref should be A, got {}", note.text);
    }

    #[test]
    fn octave_stability_no_jump() {
        // 110Hz is A — debounce on note name only so octave ambiguity doesn't prevent display.
        let (entries, _) = run_tuner(110.0, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "A", "110Hz should be A, got {}", note.text);
    }

    #[test]
    fn in_tune_when_within_5_cents() {
        let (entries, _) = run_tuner(440.0, 44100, 440.0, true);
        let cents = find_entry(&entries, "cents").expect("should have cents entry");
        assert!(
            cents.value.abs() < 5.0,
            "440Hz should be within 5 cents, got {}",
            cents.value
        );
    }
}
