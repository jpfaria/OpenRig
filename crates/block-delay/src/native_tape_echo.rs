use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, soft_saturate, DelayLine,
    MAX_DELAY_MS, MAX_FEEDBACK, MIN_DELAY_MS,
};
use crate::DelayBackendKind;

pub const MODEL_ID: &str = "tape_echo";
pub const DISPLAY_NAME: &str = "Tape Echo";

/// Hot magnetic saturation — heavier than the subtle vintage-tape model.
const SATURATION_DRIVE: f32 = 3.2;
/// Random-walk wow/flutter depth in milliseconds at full `flutter`.
const FLUTTER_MS: f32 = 6.0;
const TONE_CUTOFF_HZ: f32 = 3_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapeEchoParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub flutter: f32,
}

impl Default for TapeEchoParams {
    fn default() -> Self {
        Self {
            time_ms: 280.0,
            feedback: 40.0,
            mix: 32.0,
            flutter: 35.0,
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
                Some(TapeEchoParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(TapeEchoParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(TapeEchoParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "flutter",
                "Flutter",
                None,
                Some(TapeEchoParams::default().flutter),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<TapeEchoParams> {
    Ok(TapeEchoParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        flutter: required_f32(params, "flutter").map_err(Error::msg)? / 100.0,
    })
}

pub struct TapeEchoDelay {
    params: TapeEchoParams,
    line: DelayLine,
    tone_state: f32,
    walk: f32,
    rng: u32,
}

impl TapeEchoDelay {
    pub fn new(params: TapeEchoParams, sample_rate: f32) -> Self {
        let params = TapeEchoParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            flutter: params.flutter.clamp(0.0, 1.0),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            tone_state: 0.0,
            walk: 0.0,
            rng: 0x9E37_79B9,
        }
    }
}

impl TapeEchoDelay {
    /// Deterministic random-walk wow/flutter offset (ms). Organic, unlike a
    /// pure-sine LFO — the tape-echo signature. Seeded RNG keeps it reproducible.
    fn flutter_offset_ms(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let step = (self.rng >> 9) as f32 / (1u32 << 23) as f32 - 0.5; // [-0.5, 0.5]
        self.walk = (self.walk * 0.9995 + step * 0.05).clamp(-1.0, 1.0);
        self.walk * FLUTTER_MS * self.params.flutter
    }
}

impl MonoProcessor for TapeEchoDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sample_rate = self.line.sample_rate();
        let modulated_time = self.params.time_ms + self.flutter_offset_ms();
        self.line.set_delay_ms(modulated_time);
        let delayed = self.line.read();
        let filtered = lowpass_step(&mut self.tone_state, delayed, TONE_CUTOFF_HZ, sample_rate);
        let colored = soft_saturate(filtered, SATURATION_DRIVE);
        self.line.write(input + colored * self.params.feedback);
        mix_dry_wet(input, colored, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(TapeEchoDelay::new(
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
#[path = "native_tape_echo_tests.rs"]
mod tests;
