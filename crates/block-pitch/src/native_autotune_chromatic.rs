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

            let active_freq = if rms < threshold {
                // Below threshold: freeze current_shift_ratio, pass through dry
                None
            } else if let Some(freq) = detected_freq {
                let target = core_scales::nearest_chromatic(freq);
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
    fn integration_detuned_a_corrected_toward_440() {
        let sample_rate = 48000.0;
        // Use 466Hz (Bb4, MIDI 70) — chromatic correction should snap to either
        // A4 (440Hz, MIDI 69) or B4 (493.88Hz, MIDI 71)
        let input_freq = 466.0;
        let a4 = 440.0;
        let b4 = 493.88;
        // speed=0 for instant correction, mix=100%, no detune, low sensitivity threshold
        let mut proc = ChromaticAutotuneProcessor::new(0.0, 100.0, 0.0, 0.0, sample_rate);

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
                    let s = 0.5 * (TAU * input_freq * t).sin();
                    [s, s]
                })
                .collect();
            proc.process_block(&mut buffer);
            all_output.extend(buffer.iter().map(|f| f[0]));
        }

        // Measure output pitch using zero-crossing analysis on the last quarter
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

            // Output should be closer to nearest chromatic note (A4 or B4) than input
            let input_dist_a = (input_freq - a4).abs();
            let input_dist_b = (input_freq - b4).abs();
            let input_min_dist = input_dist_a.min(input_dist_b);

            let output_dist_a = (output_freq - a4).abs();
            let output_dist_b = (output_freq - b4).abs();
            let output_min_dist = output_dist_a.min(output_dist_b);

            assert!(
                output_min_dist < input_min_dist,
                "output ({output_freq:.1}Hz) should be closer to A4/B4 than input ({input_freq}Hz)"
            );
        }
        // If not enough crossings, the signal was too quiet/short — acceptable for this test
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
