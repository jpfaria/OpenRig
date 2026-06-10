use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, mix_dry_wet, DelayLine, MAX_DELAY_MS, MAX_FEEDBACK,
    MIN_DELAY_MS,
};
use crate::DelayBackendKind;

pub const MODEL_ID: &str = "pitch_delay";
pub const DISPLAY_NAME: &str = "Pitch Delay";

const SHIFT_WINDOW: usize = 2_048;
const MIN_SEMITONES: f32 = -24.0;
const MAX_SEMITONES: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchDelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub semitones: f32,
}

impl Default for PitchDelayParams {
    fn default() -> Self {
        Self {
            time_ms: 350.0,
            feedback: 35.0,
            mix: 35.0,
            semitones: 12.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(PitchDelayParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(PitchDelayParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(PitchDelayParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "semitones",
                "Pitch",
                None,
                Some(PitchDelayParams::default().semitones),
                MIN_SEMITONES,
                MAX_SEMITONES,
                1.0,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<PitchDelayParams> {
    Ok(PitchDelayParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        semitones: required_f32(params, "semitones").map_err(Error::msg)?,
    })
}

/// Granular dual-tap overlap-add pitch shifter (no FFT). Two read taps half a
/// window apart, Hann cross-faded, advancing at the pitch ratio.
struct PitchShifter {
    buffer: Vec<f32>,
    write_pos: usize,
    offset: f32,
    rate: f32,
}

impl PitchShifter {
    fn new(semitones: f32) -> Self {
        Self {
            buffer: vec![0.0; SHIFT_WINDOW],
            write_pos: 0,
            offset: 0.0,
            rate: 2.0_f32.powf(semitones / 12.0),
        }
    }

    fn read(&self, offset: f32) -> f32 {
        let n = self.buffer.len();
        let pos = self.write_pos as f32 + n as f32 - offset;
        let i0 = pos.floor() as usize % n;
        let i1 = (i0 + 1) % n;
        let frac = pos - pos.floor();
        (1.0 - frac).mul_add(self.buffer[i0], frac * self.buffer[i1])
    }

    fn process(&mut self, input: f32) -> f32 {
        use std::f32::consts::TAU;
        let n = SHIFT_WINDOW as f32;
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        self.offset += 1.0 - self.rate;
        self.offset = self.offset.rem_euclid(n);
        let off2 = (self.offset + n * 0.5).rem_euclid(n);

        let w1 = 0.5 - 0.5 * (TAU * self.offset / n).cos();
        let w2 = 0.5 - 0.5 * (TAU * off2 / n).cos();
        self.read(self.offset) * w1 + self.read(off2) * w2
    }
}

pub struct PitchDelay {
    params: PitchDelayParams,
    line: DelayLine,
    shifter: PitchShifter,
}

impl PitchDelay {
    pub fn new(params: PitchDelayParams, sample_rate: f32) -> Self {
        let params = PitchDelayParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            semitones: params.semitones.clamp(MIN_SEMITONES, MAX_SEMITONES),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            shifter: PitchShifter::new(params.semitones),
            params,
        }
    }
}

impl MonoProcessor for PitchDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let delayed = self.line.read();
        // Pitch-shift the repeat; with feedback the shift cascades (shimmer).
        let shifted = self.shifter.process(delayed);
        self.line.write(input + shifted * self.params.feedback);
        mix_dry_wet(input, shifted, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(PitchDelay::new(
        params_from_set(params)?,
        sample_rate,
    )))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    build_dual_mono_delay_processor(layout, || build_mono_processor(params, sample_rate))
}

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: DelayBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
#[path = "native_pitch_delay_tests.rs"]
mod tests;
