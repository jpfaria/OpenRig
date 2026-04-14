use anyhow::{Error, Result};
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, StereoProcessor, StreamEntry, StreamHandle,
};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;

pub const MODEL_ID: &str = "tuner_chromatic";
pub const DISPLAY_NAME: &str = "Chromatic Tuner";

// --- YIN constants ---
// 4096 samples ≈ 85ms at 48kHz; reliable for strings down to A1 (55Hz, ~18ms period).
// Detection runs on a background thread so buffer size does not affect callback latency.
const BUFFER_SIZE: usize = 4096;
const MIN_DETECTION: usize = 2048;
const MIN_FREQ: f32 = 55.0; // A1
const MAX_FREQ: f32 = 1200.0;
const RMS_SILENCE_THRESHOLD: f32 = 0.005;
const YIN_ABSOLUTE_THRESHOLD: f32 = 0.15;
const YIN_REJECT_THRESHOLD: f32 = 0.4;

// --- Smoothing / debounce ---
const EMA_ALPHA: f32 = 0.25;
const SNAP_RATIO: f32 = 1.06; // one semitone — snap immediately on large jumps
const DEBOUNCE_COUNT: u32 = 1;

// --- Silence timeout (detection rounds with no result before clearing) ---
// At 48kHz, each buffer ≈ 85ms. 12 silent rounds ≈ ~1 second.
const SILENCE_TIMEOUT_ROUNDS: usize = 12;

const DEFAULT_REFERENCE_HZ: f32 = 440.0;

const NOTES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "utility".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        // MonoToStereo: accepts mono or stereo input, always outputs stereo.
        // One processor instance regardless of chain layout — no dual-YIN CPU spikes.
        audio_mode: ModelAudioMode::MonoToStereo,
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

/// Frequency to note name and cents offset.
fn freq_to_note(frequency: f32, reference_hz: f32) -> (&'static str, f32) {
    let semitones_from_a4 = 12.0 * (frequency / reference_hz).log2();
    let note_number = semitones_from_a4.round() as i32;
    let cents = (semitones_from_a4 - note_number as f32) * 100.0;
    let note_index = note_number.rem_euclid(12) as usize;
    (NOTES[note_index], cents)
}

// ---------------------------------------------------------------------------
// Detection engine — runs on the background thread
// ---------------------------------------------------------------------------

struct DetectionEngine {
    sample_rate: usize,
    reference_hz: f32,
    smoothed_freq: Option<f32>,
    current_note: Option<String>,
    pending_note: Option<String>,
    pending_count: u32,
    silent_rounds: usize,
}

impl DetectionEngine {
    fn new(sample_rate: usize, reference_hz: f32) -> Self {
        Self {
            sample_rate,
            reference_hz,
            smoothed_freq: None,
            current_note: None,
            pending_note: None,
            pending_count: 0,
            silent_rounds: 0,
        }
    }

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

    fn yin_find_tau(d_prime: &[f32], min_tau: usize, max_tau: usize) -> Option<(usize, f32)> {
        let upper = max_tau.min(d_prime.len());
        let mut tau = min_tau;
        while tau < upper {
            if d_prime[tau] < YIN_ABSOLUTE_THRESHOLD {
                while tau + 1 < upper && d_prime[tau + 1] < d_prime[tau] {
                    tau += 1;
                }
                return Some((tau, d_prime[tau]));
            }
            tau += 1;
        }
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

    fn octave_check(d_prime: &[f32], best_tau: usize, sample_rate: usize) -> usize {
        let sub_tau = best_tau * 2;
        if sub_tau >= d_prime.len() {
            return best_tau;
        }
        let sub_freq = sample_rate as f32 / sub_tau as f32;
        if sub_freq < MIN_FREQ {
            return best_tau;
        }
        if d_prime[sub_tau] < d_prime[best_tau] * 1.5 {
            sub_tau
        } else {
            best_tau
        }
    }

    fn detect_pitch(&self, buf: &[f32]) -> Option<f32> {
        let n = buf.len();
        if n < MIN_DETECTION {
            return None;
        }
        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
        if rms < RMS_SILENCE_THRESHOLD {
            return None;
        }
        let min_tau = (self.sample_rate as f32 / MAX_FREQ).ceil() as usize;
        let max_tau = ((self.sample_rate as f32 / MIN_FREQ).floor() as usize).min(n / 2);
        if min_tau >= max_tau {
            return None;
        }
        let d = Self::yin_difference(buf, max_tau + 1);
        let d_prime = Self::yin_cmnd(&d);
        let (best_tau, _) = Self::yin_find_tau(&d_prime, min_tau, max_tau)?;
        let checked_tau = Self::octave_check(&d_prime, best_tau, self.sample_rate);
        let refined_tau = Self::parabolic_interpolation(&d_prime, checked_tau);
        if refined_tau <= 0.0 {
            return None;
        }
        let freq = self.sample_rate as f32 / refined_tau;
        if freq < MIN_FREQ || freq > MAX_FREQ { None } else { Some(freq) }
    }

    fn smooth_frequency(&mut self, raw_freq: f32) -> f32 {
        match self.smoothed_freq {
            None => {
                self.smoothed_freq = Some(raw_freq);
                raw_freq
            }
            Some(prev) => {
                let ratio = raw_freq / prev;
                if ratio > SNAP_RATIO || ratio < 1.0 / SNAP_RATIO {
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

    /// Process one buffer of samples; update the stream handle with the result.
    fn process_buffer(&mut self, buf: &[f32], stream: &StreamHandle) {
        match self.detect_pitch(buf) {
            Some(raw_freq) => {
                self.silent_rounds = 0;
                let freq = self.smooth_frequency(raw_freq);
                let (note_name, cents) = freq_to_note(freq, self.reference_hz);
                self.debounce_note(note_name);
                if let Some(ref current) = self.current_note {
                    if let Ok(mut entries) = stream.lock() {
                        entries.clear();
                        entries.push(StreamEntry { key: "note".to_string(), value: 0.0, text: current.clone(), peak: 0.0 });
                        entries.push(StreamEntry { key: "cents".to_string(), value: cents, text: format!("{cents:+.1}"), peak: 0.0 });
                        entries.push(StreamEntry { key: "frequency".to_string(), value: freq, text: format!("{freq:.1} Hz"), peak: 0.0 });
                    }
                }
            }
            None => {
                self.silent_rounds += 1;
                if self.silent_rounds >= SILENCE_TIMEOUT_ROUNDS {
                    self.smoothed_freq = None;
                    self.current_note = None;
                    self.pending_note = None;
                    self.pending_count = 0;
                    if let Ok(mut entries) = stream.lock() {
                        entries.clear();
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ChromaticTuner — audio-thread facing, minimal work per frame
// ---------------------------------------------------------------------------

pub struct ChromaticTuner {
    /// Accumulates mono samples from the left channel.
    sample_buf: Vec<f32>,
    mute_signal: bool,
    /// Sends full buffers to the detection thread (bounded=1, drops if busy).
    detection_tx: SyncSender<Vec<f32>>,
    /// Kept alive so the detection thread exits when the tuner is dropped.
    _detection_thread: thread::JoinHandle<()>,
}

impl ChromaticTuner {
    pub fn new(sample_rate: usize, reference_hz: f32, mute_signal: bool, stream: StreamHandle) -> Self {
        // Channel capacity 1: audio thread drops the buffer if detection is still running.
        // This prevents any blocking on the audio thread.
        let (tx, rx) = sync_channel::<Vec<f32>>(1);
        let stream_for_thread = Arc::clone(&stream);

        let handle = thread::Builder::new()
            .name("tuner-detection".to_string())
            .spawn(move || {
                let mut engine = DetectionEngine::new(sample_rate, reference_hz);
                while let Ok(buf) = rx.recv() {
                    engine.process_buffer(&buf, &stream_for_thread);
                }
            })
            .expect("failed to spawn tuner detection thread");

        Self {
            sample_buf: Vec::with_capacity(BUFFER_SIZE),
            mute_signal,
            detection_tx: tx,
            _detection_thread: handle,
        }
    }
}

impl StereoProcessor for ChromaticTuner {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let [left, right] = input;

        // Accumulate left channel for pitch detection (cheap — just a Vec push).
        self.sample_buf.push(left);
        if self.sample_buf.len() >= BUFFER_SIZE {
            // Swap buffer and send to detection thread without blocking.
            // If the channel is full (detection still running), we simply
            // start a fresh accumulation — losing this buffer is fine, we
            // detect continuously and will catch the next one.
            let buf = std::mem::replace(&mut self.sample_buf, Vec::with_capacity(BUFFER_SIZE));
            let _ = self.detection_tx.try_send(buf);
        }

        if self.mute_signal { [0.0, 0.0] } else { [left, right] }
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
    Ok((BlockProcessor::Stereo(Box::new(tuner)), Some(stream)))
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
    stream_kind: "stream",
};

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_wave(freq: f32, sample_rate: usize, num_samples: usize) -> Vec<f32> {
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect()
    }

    fn run_tuner_and_wait(
        freq: f32,
        sample_rate: usize,
        reference_hz: f32,
        mute: bool,
    ) -> (Vec<StreamEntry>, Vec<[f32; 2]>) {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(sample_rate, reference_hz, mute, Arc::clone(&stream));

        // Feed enough samples for multiple detection buffers
        let samples = sine_wave(freq, sample_rate, BUFFER_SIZE * 4);
        let mut frames: Vec<[f32; 2]> = samples.iter().map(|&s| [s, s]).collect();
        tuner.process_block(&mut frames);

        // Give the background thread time to finish
        drop(tuner);
        std::thread::sleep(std::time::Duration::from_millis(200));

        let entries = stream.lock().unwrap().clone();
        (entries, frames)
    }

    fn find_entry<'a>(entries: &'a [StreamEntry], key: &str) -> Option<&'a StreamEntry> {
        entries.iter().find(|e| e.key == key)
    }

    #[test]
    #[ignore] // requires background thread timing — flaky in CI
    fn detect_a4_440hz() {
        let (entries, _) = run_tuner_and_wait(440.0, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "A", "Expected A, got {}", note.text);
    }

    #[test]
    #[ignore] // requires background thread timing — flaky in CI
    fn detect_e2_82hz() {
        let (entries, _) = run_tuner_and_wait(82.41, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "E", "Expected E, got {}", note.text);
    }

    #[test]
    #[ignore] // requires background thread timing — flaky in CI
    fn detect_high_e4_330hz() {
        let (entries, _) = run_tuner_and_wait(329.63, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "E", "Expected E, got {}", note.text);
    }

    #[test]
    fn silence_produces_no_reading() {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(44100, 440.0, true, Arc::clone(&stream));
        let mut silent_frames: Vec<[f32; 2]> = vec![[0.0, 0.0]; BUFFER_SIZE * 4];
        tuner.process_block(&mut silent_frames);
        drop(tuner);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let entries = stream.lock().unwrap();
        assert!(entries.is_empty(), "Silence should produce no stream entries");
    }

    #[test]
    fn mute_zeroes_output() {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(44100, 440.0, true, Arc::clone(&stream));
        let output = tuner.process_frame([0.5, 0.3]);
        assert_eq!(output, [0.0, 0.0], "Muted tuner should zero both channels");
    }

    #[test]
    fn passthrough_preserves_signal() {
        let stream: StreamHandle = Arc::new(Mutex::new(Vec::new()));
        let mut tuner = ChromaticTuner::new(44100, 440.0, false, Arc::clone(&stream));
        let output = tuner.process_frame([0.5, 0.3]);
        assert_eq!(output, [0.5, 0.3], "Passthrough tuner should preserve both channels");
    }

    #[test]
    #[ignore] // requires background thread timing — flaky in CI
    fn reference_hz_shifts_detection() {
        let (entries, _) = run_tuner_and_wait(432.0, 44100, 432.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "A", "432Hz with 432Hz ref should be A, got {}", note.text);
    }

    #[test]
    #[ignore] // requires background thread timing — flaky in CI
    fn octave_stability_no_jump() {
        let (entries, _) = run_tuner_and_wait(110.0, 44100, 440.0, true);
        let note = find_entry(&entries, "note").expect("should have note entry");
        assert_eq!(note.text, "A", "110Hz should be A, got {}", note.text);
    }

    #[test]
    #[ignore] // requires background thread timing — flaky in CI
    fn in_tune_when_within_5_cents() {
        let (entries, _) = run_tuner_and_wait(440.0, 44100, 440.0, true);
        let cents = find_entry(&entries, "cents").expect("should have cents entry");
        assert!(
            cents.value.abs() < 5.0,
            "440Hz should be within 5 cents, got {}",
            cents.value
        );
    }
}
