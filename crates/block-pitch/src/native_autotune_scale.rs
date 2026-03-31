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
            float_parameter(
                "key",
                "Key",
                None,
                Some(0.0),
                0.0,
                11.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "scale",
                "Scale",
                None,
                Some(0.0),
                0.0,
                7.0,
                1.0,
                ParameterUnit::None,
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

            let (active_freq, target_ratio) = if rms < threshold {
                (None, 1.0)
            } else if let Some(freq) = detected_freq {
                let target = core_scales::nearest_in_scale(freq, self.key, self.scale);
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
    let key = required_f32(params, "key").map_err(Error::msg)? as u8;
    let scale = required_f32(params, "scale").map_err(Error::msg)? as u8;

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
}
