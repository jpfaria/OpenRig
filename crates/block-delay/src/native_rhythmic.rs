use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, DelayLine, MAX_DELAY_MS,
    MAX_FEEDBACK, MIN_DELAY_MS,
};
use crate::DelayBackendKind;

pub const MODEL_ID: &str = "rhythmic";
pub const DISPLAY_NAME: &str = "Rhythmic Delay";

/// The `time` knob is the beat (quarter note); the subdivision applies a
/// musical ratio so the echoes fall on a syncopated grid.
const SUBDIVISION_RATIOS: [f32; 3] = [0.75, 2.0 / 3.0, 0.5]; // dotted-8th, triplet, 8th
const TONE_CUTOFF_HZ: f32 = 6_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RhythmicParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    /// 0 = dotted-eighth, 1 = triplet, 2 = eighth.
    pub subdivision: f32,
}

impl Default for RhythmicParams {
    fn default() -> Self {
        Self {
            time_ms: 500.0,
            feedback: 38.0,
            mix: 32.0,
            subdivision: 0.0,
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
                "Beat",
                None,
                Some(RhythmicParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(RhythmicParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(RhythmicParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "subdivision",
                "Subdivision",
                None,
                Some(RhythmicParams::default().subdivision),
                0.0,
                2.0,
                1.0,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<RhythmicParams> {
    Ok(RhythmicParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        subdivision: required_f32(params, "subdivision").map_err(Error::msg)?,
    })
}

fn subdivision_ratio(subdivision: f32) -> f32 {
    let idx = (subdivision.round() as usize).min(SUBDIVISION_RATIOS.len() - 1);
    SUBDIVISION_RATIOS[idx]
}

pub struct RhythmicDelay {
    params: RhythmicParams,
    line: DelayLine,
    tone_state: f32,
}

impl RhythmicDelay {
    pub fn new(params: RhythmicParams, sample_rate: f32) -> Self {
        let params = RhythmicParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            subdivision: params.subdivision.clamp(0.0, 2.0),
        };
        // The echo grid is the beat scaled by the musical subdivision.
        let effective_time = clamp_time_ms(params.time_ms * subdivision_ratio(params.subdivision));
        Self {
            line: DelayLine::new(effective_time, sample_rate),
            params,
            tone_state: 0.0,
        }
    }
}

impl MonoProcessor for RhythmicDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sample_rate = self.line.sample_rate();
        let delayed = self.line.read();
        let filtered = lowpass_step(&mut self.tone_state, delayed, TONE_CUTOFF_HZ, sample_rate);
        self.line.write(input + filtered * self.params.feedback);
        mix_dry_wet(input, filtered, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(RhythmicDelay::new(
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
#[path = "native_rhythmic_tests.rs"]
mod tests;
