use anyhow::{Error, Result};
use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, MonoProcessor, StreamEntry, StreamHandle};
use std::sync::{Arc, Mutex};
use block_core::ModelAudioMode;

pub const MODEL_ID: &str = "tuner_chromatic";
pub const DISPLAY_NAME: &str = "Chromatic Tuner";
const DEFAULT_REFERENCE_HZ: f32 = 440.0;
const BUFFER_SIZE: usize = 2048;
const A1_HZ: f32 = 65.0;   // C2 — lowest useful guitar note
const E6_HZ: f32 = 1000.0; // Narrower range for faster detection

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "utility".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Chromatic Tuner".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![float_parameter(
            "reference_hz",
            "Reference",
            None,
            Some(DEFAULT_REFERENCE_HZ),
            400.0,
            480.0,
            1.0,
            ParameterUnit::Hertz,
        )],
    }
}

pub fn reference_hz_from_set(params: &ParameterSet) -> Result<f32> {
    required_f32(params, "reference_hz").map_err(Error::msg)
}

pub struct ChromaticTuner {
    buffer: Vec<f32>,
    sample_rate: usize,
    stream: StreamHandle,
}

impl ChromaticTuner {
    pub fn new(sample_rate: usize, stream: StreamHandle) -> Self {
        Self {
            buffer: Vec::with_capacity(BUFFER_SIZE),
            sample_rate,
            stream,
        }
    }

    fn simple_amdf(&self) -> Option<f32> {
        if self.buffer.is_empty() {
            return None;
        }
        let rms = (self
            .buffer
            .iter()
            .map(|sample| sample * sample)
            .sum::<f32>()
            / self.buffer.len() as f32)
            .sqrt();
        if rms < 0.01 {
            return None;
        }
        let min_period = (self.sample_rate as f32 / E6_HZ) as usize;
        let max_period = (self.sample_rate as f32 / A1_HZ) as usize;
        let mut best_period = 0;
        let mut min_diff = f32::MAX;
        for lag in min_period..max_period.min(self.buffer.len() / 2) {
            let mut diff = 0.0;
            for index in 0..(self.buffer.len() - lag) {
                diff += (self.buffer[index] - self.buffer[index + lag]).abs();
            }
            if diff < min_diff {
                min_diff = diff;
                best_period = lag;
            }
        }
        if best_period > 0 {
            Some(self.sample_rate as f32 / best_period as f32)
        } else {
            None
        }
    }
}

impl MonoProcessor for ChromaticTuner {
    fn process_sample(&mut self, input: f32) -> f32 {
        // Accumulate samples for analysis
        self.buffer.push(input);
        if self.buffer.len() > BUFFER_SIZE * 2 {
            // Trim old samples, keep the latest BUFFER_SIZE
            let start = self.buffer.len() - BUFFER_SIZE;
            self.buffer.drain(..start);
        }
        // Pass signal through unmodified (tuner is non-blocking)
        input
    }

    fn process_block(&mut self, buffer: &mut [f32]) {
        // Accumulate all samples in the block
        self.buffer.extend_from_slice(buffer);
        if self.buffer.len() > BUFFER_SIZE * 2 {
            let start = self.buffer.len() - BUFFER_SIZE;
            self.buffer.drain(..start);
        }

        // Run detection when we have enough samples
        if self.buffer.len() >= BUFFER_SIZE {
            if let Some(frequency) = self.simple_amdf() {
                let (note, octave, cents) = freq_to_note(frequency);
                let mut entries = self.stream.lock().unwrap();
                entries.clear();
                entries.push(StreamEntry {
                    key: "frequency".to_string(),
                    value: frequency,
                    text: format!("{}{}", note, octave),
                });
                entries.push(StreamEntry {
                    key: "cents_off".to_string(),
                    value: cents,
                    text: format!("{:+.1}", cents),
                });
                entries.push(StreamEntry {
                    key: "in_tune".to_string(),
                    value: if cents.abs() < 5.0 { 1.0 } else { 0.0 },
                    text: if cents.abs() < 5.0 { "in_tune" } else { "out" }.to_string(),
                });
            }
            self.buffer.clear();
        }
        // Pass signal through unmodified
    }
}
fn freq_to_note(frequency: f32) -> (&'static str, i32, f32) {
    let semitones_from_a4 = 12.0 * (frequency / 440.0).log2();
    let note_number = semitones_from_a4.round() as i32;
    let cents = (semitones_from_a4 - note_number as f32) * 100.0;
    const NOTES: [&str; 12] = [
        "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
    ];
    let note_index = note_number.rem_euclid(12) as usize;
    let octave = 4 + (note_number + 9) / 12;
    (NOTES[note_index], octave, cents)
}
fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: usize,
    _layout: AudioChannelLayout,
) -> Result<(BlockProcessor, Option<StreamHandle>)> {
    let _reference_hz = reference_hz_from_set(params)?;
    let stream = Arc::new(Mutex::new(Vec::new()));
    let tuner = ChromaticTuner::new(sample_rate, stream.clone());
    Ok((BlockProcessor::Mono(Box::new(tuner)), Some(stream)))
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

    #[test]
    fn test_freq_to_note() {
        let (note, octave, cents) = freq_to_note(440.0);
        assert_eq!(note, "A");
        assert_eq!(octave, 4);
        assert!(cents.abs() < 0.5);
    }
}
