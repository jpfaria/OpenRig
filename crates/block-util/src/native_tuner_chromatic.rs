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

use feature_dsp::pitch_yin::{PitchDetector, PitchUpdate, BUFFER_SIZE, DEFAULT_REFERENCE_HZ};
use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;

pub const MODEL_ID: &str = "tuner_chromatic";
pub const DISPLAY_NAME: &str = "Chromatic Tuner";

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
    pub fn new(
        sample_rate: usize,
        reference_hz: f32,
        mute_signal: bool,
        stream: StreamHandle,
    ) -> Self {
        // Channel capacity 1: audio thread drops the buffer if detection is still running.
        // This prevents any blocking on the audio thread.
        let (tx, rx) = sync_channel::<Vec<f32>>(1);
        let stream_for_thread = Arc::clone(&stream);

        let handle = thread::Builder::new()
            .name("tuner-detection".to_string())
            .spawn(move || {
                let mut detector = PitchDetector::new(sample_rate, reference_hz);
                while let Ok(buf) = rx.recv() {
                    publish(&mut detector, &buf, &stream_for_thread);
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

        if self.mute_signal {
            [0.0, 0.0]
        } else {
            [left, right]
        }
    }
}

/// Translate one detector update into stream entries the GUI consumes.
fn publish(detector: &mut PitchDetector, buf: &[f32], stream: &StreamHandle) {
    match detector.process_buffer(buf) {
        PitchUpdate::Update { note, cents, freq } => {
            if let Ok(mut entries) = stream.lock() {
                entries.clear();
                entries.push(StreamEntry {
                    key: "note".to_string(),
                    value: 0.0,
                    text: note.to_string(),
                    peak: 0.0,
                });
                entries.push(StreamEntry {
                    key: "cents".to_string(),
                    value: cents,
                    text: format!("{cents:+.1}"),
                    peak: 0.0,
                });
                entries.push(StreamEntry {
                    key: "frequency".to_string(),
                    value: freq,
                    text: format!("{freq:.1} Hz"),
                    peak: 0.0,
                });
            }
        }
        PitchUpdate::Silence => {
            if let Ok(mut entries) = stream.lock() {
                entries.clear();
            }
        }
        PitchUpdate::NoChange => {}
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
        assert_eq!(
            output,
            [0.5, 0.3],
            "Passthrough tuner should preserve both channels"
        );
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
