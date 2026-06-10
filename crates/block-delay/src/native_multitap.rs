use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::DelayBackendKind;
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, mix_dry_wet, sanitize, MAX_DELAY_MS, MAX_FEEDBACK,
    MIN_DELAY_MS,
};

pub const MODEL_ID: &str = "multitap";
pub const DISPLAY_NAME: &str = "Multi-Tap Delay";

const MIN_TAPS: f32 = 2.0;
const MAX_TAPS: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultiTapParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub taps: f32,
}

impl Default for MultiTapParams {
    fn default() -> Self {
        Self {
            time_ms: 400.0,
            feedback: 25.0,
            mix: 35.0,
            taps: 4.0,
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
                Some(MultiTapParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(MultiTapParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(MultiTapParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "taps",
                "Taps",
                None,
                Some(MultiTapParams::default().taps),
                MIN_TAPS,
                MAX_TAPS,
                1.0,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<MultiTapParams> {
    Ok(MultiTapParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        taps: required_f32(params, "taps").map_err(Error::msg)?,
    })
}

pub struct MultiTapDelay {
    params: MultiTapParams,
    buffer: Vec<f32>,
    write_pos: usize,
    /// Read offset (samples) and normalized gain per tap, precomputed so
    /// `process_sample` does no allocation.
    tap_offsets: Vec<usize>,
    tap_gains: Vec<f32>,
    /// The longest tap (the full base time) feeds the recirculation.
    feedback_offset: usize,
}

impl MultiTapDelay {
    pub fn new(params: MultiTapParams, sample_rate: f32) -> Self {
        let params = MultiTapParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            taps: params.taps.clamp(MIN_TAPS, MAX_TAPS),
        };
        let max_len = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + 2;
        let tap_count = params.taps.round() as usize;

        // Taps land on even sub-divisions of the base time: time/tap_count,
        // 2·time/tap_count, … , time. Gains fall off as 1/i and are normalized
        // so the summed wet level stays bounded.
        let raw_gains: Vec<f32> = (1..=tap_count).map(|i| 1.0 / i as f32).collect();
        let gain_sum: f32 = raw_gains.iter().sum();
        let tap_gains: Vec<f32> = raw_gains.iter().map(|g| g / gain_sum).collect();
        let tap_offsets: Vec<usize> = (1..=tap_count)
            .map(|i| {
                let frac = i as f32 / tap_count as f32;
                ((params.time_ms * frac * 0.001 * sample_rate).round() as usize)
                    .clamp(1, max_len - 1)
            })
            .collect();
        let feedback_offset = *tap_offsets.last().unwrap_or(&1);

        Self {
            params,
            buffer: vec![0.0; max_len],
            write_pos: 0,
            tap_offsets,
            tap_gains,
            feedback_offset,
        }
    }

    fn read(&self, delay_samples: usize) -> f32 {
        let len = self.buffer.len();
        self.buffer[(self.write_pos + len - delay_samples) % len]
    }
}

impl MonoProcessor for MultiTapDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut wet = 0.0;
        for (offset, gain) in self.tap_offsets.iter().zip(self.tap_gains.iter()) {
            wet += self.read(*offset) * gain;
        }
        let feedback_sample = self.read(self.feedback_offset);

        self.buffer[self.write_pos] = sanitize(input + feedback_sample * self.params.feedback);
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        mix_dry_wet(input, wet, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(MultiTapDelay::new(
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
#[path = "native_multitap_tests.rs"]
mod tests;
