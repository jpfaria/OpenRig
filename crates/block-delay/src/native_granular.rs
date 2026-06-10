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

pub const MODEL_ID: &str = "granular";
pub const DISPLAY_NAME: &str = "Granular Delay";

const GRAIN_SIZE: usize = 2_048;
/// Max random position jitter (samples) at full `spread`.
const MAX_SPREAD_SAMPLES: f32 = 6_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GranularParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub spread: f32,
}

impl Default for GranularParams {
    fn default() -> Self {
        Self {
            time_ms: 300.0,
            feedback: 30.0,
            mix: 40.0,
            spread: 45.0,
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
                Some(GranularParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(GranularParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(GranularParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "spread",
                "Spread",
                None,
                Some(GranularParams::default().spread),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<GranularParams> {
    Ok(GranularParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        spread: required_f32(params, "spread").map_err(Error::msg)? / 100.0,
    })
}

struct Grain {
    read_pos: usize,
    age: usize,
}

pub struct GranularDelay {
    params: GranularParams,
    buffer: Vec<f32>,
    write_pos: usize,
    base_delay: usize,
    spread_samples: usize,
    grains: [Grain; 2],
    rng: u32,
}

impl GranularDelay {
    pub fn new(params: GranularParams, sample_rate: f32) -> Self {
        let params = GranularParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            spread: params.spread.clamp(0.0, 1.0),
        };
        let max_len = (MAX_DELAY_MS * 0.001 * sample_rate) as usize + GRAIN_SIZE + 2;
        let base_delay =
            ((params.time_ms * 0.001 * sample_rate) as usize).clamp(1, max_len - GRAIN_SIZE - 1);
        let spread_samples = (params.spread * MAX_SPREAD_SAMPLES) as usize;
        GranularDelay {
            params,
            buffer: vec![0.0; max_len],
            write_pos: 0,
            base_delay,
            spread_samples,
            // Two grains offset by half a grain → continuous Hann overlap-add.
            grains: [
                Grain { read_pos: 0, age: 0 },
                Grain {
                    read_pos: 0,
                    age: GRAIN_SIZE / 2,
                },
            ],
            rng: 0x1234_5678,
        }
    }
}

impl MonoProcessor for GranularDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        use std::f32::consts::TAU;
        let len = self.buffer.len();
        let mut rng = self.rng;
        let mut wet = 0.0;

        for grain in &mut self.grains {
            if grain.age >= GRAIN_SIZE {
                // Respawn at the base delay plus a random offset → each grain
                // re-reads the recorded signal at a slightly different time,
                // which is what scatters a sound into a granular cloud.
                let jitter = if self.spread_samples > 0 {
                    rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (rng >> 8) as usize % self.spread_samples
                } else {
                    0
                };
                let offset = (self.base_delay + jitter).min(len - 1);
                grain.read_pos = (self.write_pos + len - offset) % len;
                grain.age = 0;
            }
            // Hann window over the grain so overlapping grains cross-fade.
            let window = 0.5 - 0.5 * (TAU * grain.age as f32 / GRAIN_SIZE as f32).cos();
            wet += self.buffer[grain.read_pos] * window;
            grain.read_pos = (grain.read_pos + 1) % len;
            grain.age += 1;
        }

        self.rng = rng;
        self.buffer[self.write_pos] = sanitize(input + wet * self.params.feedback);
        self.write_pos = (self.write_pos + 1) % len;
        mix_dry_wet(input, wet, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(GranularDelay::new(
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
#[path = "native_granular_tests.rs"]
mod tests;
