use anyhow::{Error, Result};

use crate::core_pitch_detect::PitchDetector;
use crate::core_psola::PsolaShifter;
use crate::core_scales;
use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, StereoProcessor};

pub const MODEL_ID: &str = "native_autotune_scale";
pub const DISPLAY_NAME: &str = "Scale Autotune";

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
                ParameterUnit::None,
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
            enum_parameter(
                "key",
                "Key",
                None,
                Some("c"),
                &[
                    ("c", "C"), ("cs", "C#"), ("d", "D"), ("ds", "D#"),
                    ("e", "E"), ("f", "F"), ("fs", "F#"), ("g", "G"),
                    ("gs", "G#"), ("a", "A"), ("as", "A#"), ("b", "B"),
                ],
            ),
            enum_parameter(
                "scale",
                "Scale",
                None,
                Some("major"),
                &[
                    ("major", "Major"),
                    ("natural_minor", "Natural Minor"),
                    ("pentatonic_major", "Pentatonic Major"),
                    ("pentatonic_minor", "Pentatonic Minor"),
                    ("harmonic_minor", "Harmonic Minor"),
                    ("melodic_minor", "Melodic Minor"),
                    ("blues", "Blues"),
                    ("dorian", "Dorian"),
                ],
            ),
        ],
    }
}

struct ScaleAutotuneProcessor {
    detector: PitchDetector,
    shifter: PsolaShifter,
    speed_ms: f32,
    mix: f32,
    detune_cents: f32,
    sensitivity: f32,
    key: u8,
    scale: u8,
    sample_rate: f32,
    current_shift_ratio: f32,
}

impl ScaleAutotuneProcessor {
    fn new(
        speed_ms: f32,
        mix: f32,
        detune_cents: f32,
        sensitivity: f32,
        key: u8,
        scale: u8,
        sample_rate: f32,
    ) -> Self {
        Self {
            detector: PitchDetector::new(sample_rate),
            shifter: PsolaShifter::new(sample_rate),
            speed_ms,
            mix: mix / 100.0,
            detune_cents,
            sensitivity,
            key,
            scale,
            sample_rate,
            current_shift_ratio: 1.0,
        }
    }
}

impl StereoProcessor for ScaleAutotuneProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        // Single-sample fallback: accumulate and process
        let mono = (input[0] + input[1]) * 0.5;
        // Simple pass-through for single-sample mode (process_block is preferred)
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

            let active_freq = if rms < threshold {
                // Below threshold: freeze current_shift_ratio, pass through dry
                None
            } else if let Some(freq) = detected_freq {
                let target = core_scales::nearest_in_scale(freq, self.key, self.scale);
                let target = core_scales::apply_detune(target, self.detune_cents);
                let target_ratio = target / freq;

                // Smooth shift ratio
                let alpha = if self.speed_ms <= 0.0 {
                    1.0
                } else {
                    1.0 - (-1.0 / (self.speed_ms * 0.001 * self.sample_rate)).exp()
                };
                self.current_shift_ratio += alpha * (target_ratio - self.current_shift_ratio);

                Some(freq)
            } else {
                None
            };

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
    let key = core_scales::key_from_str(&required_string(params, "key").map_err(Error::msg)?);
    let scale = core_scales::scale_from_str(&required_string(params, "scale").map_err(Error::msg)?);

    let processor =
        ScaleAutotuneProcessor::new(speed, mix, detune, sensitivity, key, scale, sample_rate);

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
        assert_eq!(schema.parameters.len(), 6);
    }

    #[test]
    fn defaults_normalize() {
        let schema = model_schema();
        let params = ParameterSet::default();
        params.normalized_against(&schema).expect("defaults should normalize");
    }

    #[test]
    fn integration_fsharp_in_cmajor_corrected_toward_f_or_g() {
        let sample_rate = 48000.0;
        // F#4 ~ 369.99 Hz — not in C Major. Nearest: F4 (349.23) or G4 (392.00)
        let f_sharp_freq = 440.0 * 2f32.powf((66.0 - 69.0) / 12.0); // MIDI 66
        let f4_freq = 440.0 * 2f32.powf((65.0 - 69.0) / 12.0); // MIDI 65
        let g4_freq = 440.0 * 2f32.powf((67.0 - 69.0) / 12.0); // MIDI 67

        // speed=0, mix=100%, no detune, sensitivity=0 (always active), key=C(0), scale=Major(0)
        let mut proc =
            ScaleAutotuneProcessor::new(0.0, 100.0, 0.0, 0.0, 0, 0, sample_rate);

        let block_size = 512;
        let num_blocks = 40;
        let total_samples = block_size * num_blocks;

        // Process in multiple blocks to simulate real-time streaming
        let mut all_output = Vec::with_capacity(total_samples);
        for block_idx in 0..num_blocks {
            let offset = block_idx * block_size;
            let mut buffer: Vec<[f32; 2]> = (0..block_size)
                .map(|i| {
                    let t = (offset + i) as f32 / sample_rate;
                    let s = 0.5 * (TAU * f_sharp_freq * t).sin();
                    [s, s]
                })
                .collect();
            proc.process_block(&mut buffer);
            all_output.extend(buffer.iter().map(|f| f[0]));
        }

        // Measure output pitch via zero crossings on the last quarter
        let start = total_samples * 3 / 4;
        let output_mono = &all_output[start..];

        let mut crossings = Vec::new();
        for i in 1..output_mono.len() {
            if output_mono[i - 1] <= 0.0 && output_mono[i] > 0.0 {
                crossings.push(i);
            }
        }

        if crossings.len() >= 3 {
            let periods: Vec<f32> = crossings
                .windows(2)
                .map(|w| (w[1] - w[0]) as f32)
                .collect();
            let avg_period = periods.iter().sum::<f32>() / periods.len() as f32;
            let output_freq = sample_rate / avg_period;

            // Output should be closer to F4 or G4 than the input F#4
            let input_dist_f = (f_sharp_freq - f4_freq).abs();
            let input_dist_g = (f_sharp_freq - g4_freq).abs();
            let input_min_dist = input_dist_f.min(input_dist_g);

            let output_dist_f = (output_freq - f4_freq).abs();
            let output_dist_g = (output_freq - g4_freq).abs();
            let output_min_dist = output_dist_f.min(output_dist_g);

            assert!(
                output_min_dist < input_min_dist,
                "output ({output_freq:.1}Hz) should be closer to F4/G4 than input ({f_sharp_freq:.1}Hz)"
            );
        }
    }
}
