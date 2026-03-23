use anyhow::{Error, Result};
use crate::processor::{TunerProcessor, TunerReading};
use crate::registry::UtilModelDefinition;
use crate::UtilBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::ModelAudioMode;

pub const MODEL_ID: &str = "tuner_chromatic";
pub const DISPLAY_NAME: &str = "Chromatic Tuner";
const DEFAULT_REFERENCE_HZ: f32 = 440.0;
const BUFFER_SIZE: usize = 4096;
const A1_HZ: f32 = 50.1;
const E6_HZ: f32 = 1245.0;

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
    reading: TunerReading,
    enabled: bool,
}

impl ChromaticTuner {
    pub fn new(sample_rate: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(BUFFER_SIZE),
            sample_rate,
            reading: TunerReading::default(),
            enabled: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.buffer.clear();
            self.reading = TunerReading::default();
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

impl TunerProcessor for ChromaticTuner {
    fn process(&mut self, samples: &[f32]) {
        if !self.enabled {
            return;
        }
        self.buffer.extend_from_slice(samples);
        if self.buffer.len() >= BUFFER_SIZE {
            let detected_frequency = self.simple_amdf();
            self.reading = detected_frequency.into();
            self.buffer.clear();
        }
    }

    fn latest_reading(&self) -> &TunerReading {
        &self.reading
    }
}

impl From<Option<f32>> for TunerReading {
    fn from(frequency: Option<f32>) -> Self {
        match frequency {
            None => Self::default(),
            Some(frequency) => {
                let (note, octave, cents) = freq_to_note(frequency);
                Self {
                    frequency: Some(frequency),
                    note: Some(format!("{note}{octave}")),
                    cents_off: Some(cents),
                    in_tune: cents.abs() < 5.0,
                }
            }
        }
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

fn build(params: &ParameterSet, sample_rate: usize) -> Result<Box<dyn TunerProcessor>> {
    let _reference_hz = reference_hz_from_set(params)?;
    let mut tuner = ChromaticTuner::new(sample_rate);
    tuner.set_enabled(true);
    Ok(Box::new(tuner))
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
    fn detect_reference_pitch_name() {
        let info = TunerReading::from(Some(440.0));
        assert_eq!(info.note.as_deref(), Some("A4"));
        assert!(info.in_tune);
    }
}
