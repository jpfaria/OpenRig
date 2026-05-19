use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, StereoProcessor};

use crate::pitch_engine::PitchEngine;
use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;

pub const MODEL_ID: &str = "native_pitch_shifter";
pub const DISPLAY_NAME: &str = "Pitch Shifter";

const GRAIN_LEN: usize = 1024;
const MIN_SEMITONES: f32 = -24.0;
const MAX_SEMITONES: f32 = 24.0;
const MIN_CENTS: f32 = -100.0;
const MAX_CENTS: f32 = 100.0;

#[derive(Debug, Clone, Copy)]
struct Params {
    shift_semitones: f32,
    shift_cents: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            shift_semitones: 0.0,
            shift_cents: 0.0,
            mix: 1.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let d = Params::default();
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_PITCH.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "shift_semitones",
                "Semitones",
                None,
                Some(d.shift_semitones),
                MIN_SEMITONES,
                MAX_SEMITONES,
                1.0,
                ParameterUnit::Semitones,
            ),
            float_parameter(
                "shift_cents",
                "Cents",
                None,
                Some(d.shift_cents),
                MIN_CENTS,
                MAX_CENTS,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(d.mix * 100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        shift_semitones: required_f32(params, "shift_semitones")
            .map_err(Error::msg)?
            .clamp(MIN_SEMITONES, MAX_SEMITONES),
        shift_cents: required_f32(params, "shift_cents")
            .map_err(Error::msg)?
            .clamp(MIN_CENTS, MAX_CENTS),
        mix: (required_f32(params, "mix").map_err(Error::msg)? / 100.0).clamp(0.0, 1.0),
    })
}

fn semitones_to_pitch_factor(semitones: f32, cents: f32) -> f32 {
    2.0_f32.powf((semitones + cents / 100.0) / 12.0)
}

struct PitchShifter {
    voc_l: PitchEngine,
    voc_r: PitchEngine,
    mix: f32,
}

impl PitchShifter {
    fn new(params: Params) -> Self {
        let factor = semitones_to_pitch_factor(params.shift_semitones, params.shift_cents);
        let mut voc_l = PitchEngine::new(GRAIN_LEN);
        let mut voc_r = PitchEngine::new(GRAIN_LEN);
        voc_l.set_pitch_factor(factor);
        voc_r.set_pitch_factor(factor);
        Self {
            voc_l,
            voc_r,
            mix: params.mix.clamp(0.0, 1.0),
        }
    }
}

impl StereoProcessor for PitchShifter {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let wet_l = self.voc_l.process_sample(input[0]);
        let wet_r = self.voc_r.process_sample(input[1]);
        let dry = 1.0 - self.mix;
        [
            dry.mul_add(input[0], self.mix * wet_l),
            dry.mul_add(input[1], self.mix * wet_r),
        ]
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    _sample_rate: f32,
    _layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = params_from_set(params)?;
    Ok(BlockProcessor::Stereo(Box::new(PitchShifter::new(p))))
}

pub const MODEL_DEFINITION: PitchModelDefinition = PitchModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: PitchBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
#[path = "native_pitch_shifter_tests.rs"]
mod tests;
