use anyhow::{Error, Result};

use crate::core_pitch_detect::PitchDetector;
use crate::core_psola::PsolaShifter;
use crate::core_scales;
use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, StereoProcessor};

pub const MODEL_ID: &str = "native_autotune_chromatic";
pub const DISPLAY_NAME: &str = "Chromatic Autotune";

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "speed",
                "Speed",
                None,
                Some(20.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "detune",
                "Detune",
                None,
                Some(0.0),
                -50.0,
                50.0,
                1.0,
                ParameterUnit::Semitones,
            ),
            float_parameter(
                "sensitivity",
                "Sensitivity",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

struct ChromaticAutotuneProcessor {
    detector: PitchDetector,
    shifter: PsolaShifter,
    speed_ms: f32,
    mix: f32,
    detune_cents: f32,
    sensitivity: f32,
    sample_rate: f32,
    current_shift_ratio: f32,
}

impl ChromaticAutotuneProcessor {
    fn new(
        speed_ms: f32,
        mix: f32,
        detune_cents: f32,
        sensitivity: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            detector: PitchDetector::new(sample_rate),
            shifter: PsolaShifter::new(sample_rate),
            speed_ms,
            mix: mix / 100.0,
            detune_cents,
            sensitivity,
            sample_rate,
            current_shift_ratio: 1.0,
        }
    }
}

impl StereoProcessor for ChromaticAutotuneProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        // Fallback single-sample: just downmix (process_block is the real path)
        let mono = (input[0] + input[1]) * 0.5;
        [mono, mono]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        let len = buffer.len();
        let block_size = 256;
        let mut pos = 0;

        while pos < len {
            let remaining = len - pos;
            let chunk = remaining.min(block_size);

            // Downmix to mono
            let mut mono_input = vec![0.0f32; chunk];
            for (i, frame) in buffer[pos..pos + chunk].iter().enumerate() {
                mono_input[i] = (frame[0] + frame[1]) * 0.5;
            }

            // Feed pitch detector
            for &s in &mono_input {
                self.detector.push_sample(s);
            }

            let detected_freq = self.detector.detect();

            // Sensitivity gate
            let rms =
                (mono_input.iter().map(|s| s * s).sum::<f32>() / chunk as f32).sqrt();
            let threshold = self.sensitivity / 100.0 * 0.1;

            let (active_freq, target_ratio) = if rms < threshold {
                (None, 1.0)
            } else if let Some(freq) = detected_freq {
                let target = core_scales::nearest_chromatic(freq);
                let target = core_scales::apply_detune(target, self.detune_cents);
                (Some(freq), target / freq)
            } else {
                (None, 1.0)
            };

            // Smooth shift ratio
            let alpha = if self.speed_ms <= 0.0 {
                1.0
            } else {
                1.0 - (-1.0 / (self.speed_ms * 0.001 * self.sample_rate)).exp()
            };
            self.current_shift_ratio += alpha * (target_ratio - self.current_shift_ratio);

            // PSOLA
            let mut mono_output = vec![0.0f32; chunk];
            self.shifter.process_block(
                &mono_input,
                &mut mono_output,
                active_freq,
                self.current_shift_ratio,
            );

            // Mix and duplicate to stereo
            for (i, frame) in buffer[pos..pos + chunk].iter_mut().enumerate() {
                let dry = mono_input[i];
                let wet = mono_output[i];
                let mixed = dry * (1.0 - self.mix) + wet * self.mix;
                *frame = [mixed, mixed];
            }

            pos += chunk;
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    let speed = required_f32(params, "speed").map_err(Error::msg)?;
    let mix = required_f32(params, "mix").map_err(Error::msg)?;
    let detune = required_f32(params, "detune").map_err(Error::msg)?;
    let sensitivity = required_f32(params, "sensitivity").map_err(Error::msg)?;

    let processor = ChromaticAutotuneProcessor::new(speed, mix, detune, sensitivity, sample_rate);

    match layout {
        block_core::AudioChannelLayout::Stereo | block_core::AudioChannelLayout::Mono => {
            Ok(block_core::BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

pub const MODEL_DEFINITION: PitchModelDefinition = PitchModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: PitchBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::StereoProcessor;
    use std::f32::consts::TAU;

    #[test]
    fn schema_is_valid() {
        let schema = model_schema();
        assert_eq!(schema.effect_type, "pitch");
        assert_eq!(schema.model, MODEL_ID);
        assert_eq!(schema.parameters.len(), 4);
    }

    #[test]
    fn sensitivity_gate_passes_through_quiet_signal() {
        let sample_rate = 48000.0;
        let mut proc = ChromaticAutotuneProcessor::new(
            0.0,
            100.0,
            0.0,
            50.0, // sensitivity at 50% -> threshold = 0.05
            sample_rate,
        );

        // Very quiet signal
        let amplitude = 0.001;
        let block_size = 512;
        let mut buffer: Vec<[f32; 2]> = (0..block_size)
            .map(|i| {
                let t = i as f32 / sample_rate;
                let s = amplitude * (TAU * 440.0 * t).sin();
                [s, s]
            })
            .collect();

        let input_copy: Vec<[f32; 2]> = buffer.clone();
        proc.process_block(&mut buffer);

        // Below sensitivity threshold -> pass-through (no pitch correction applied)
        for (i, (out, inp)) in buffer.iter().zip(input_copy.iter()).enumerate() {
            let mono_in = (inp[0] + inp[1]) * 0.5;
            assert!(
                (out[0] - mono_in).abs() < amplitude * 2.0,
                "frame {i}: expected ~{mono_in}, got {}",
                out[0]
            );
        }
    }

    #[test]
    fn mix_zero_returns_dry() {
        let sample_rate = 48000.0;
        let mut proc = ChromaticAutotuneProcessor::new(
            0.0,
            0.0, // mix = 0% -> all dry
            0.0,
            0.0,
            sample_rate,
        );

        let block_size = 512;
        let mut buffer: Vec<[f32; 2]> = (0..block_size)
            .map(|i| {
                let t = i as f32 / sample_rate;
                let s = 0.5 * (TAU * 440.0 * t).sin();
                [s, s]
            })
            .collect();

        let input_copy: Vec<[f32; 2]> = buffer.clone();
        proc.process_block(&mut buffer);

        // mix=0% -> output = dry mono downmix
        for (i, (out, inp)) in buffer.iter().zip(input_copy.iter()).enumerate() {
            let mono_in = (inp[0] + inp[1]) * 0.5;
            assert!(
                (out[0] - mono_in).abs() < 0.01,
                "frame {i}: expected ~{mono_in}, got {}",
                out[0]
            );
        }
    }
}
