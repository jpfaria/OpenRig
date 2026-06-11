use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, StereoProcessor};

use crate::registry::{build_stereo_delay_processor, DelayModelDefinition};
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, mix_dry_wet, DelayLine, MAX_DELAY_MS, MAX_FEEDBACK,
    MIN_DELAY_MS,
};
use crate::DelayBackendKind;

pub const MODEL_ID: &str = "ping_pong";
pub const DISPLAY_NAME: &str = "Ping-Pong Delay";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PingPongParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for PingPongParams {
    fn default() -> Self {
        Self {
            time_ms: 300.0,
            feedback: 40.0,
            mix: 35.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(PingPongParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(PingPongParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(PingPongParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<PingPongParams> {
    Ok(PingPongParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct PingPongDelay {
    params: PingPongParams,
    left_line: DelayLine,
    right_line: DelayLine,
}

impl PingPongDelay {
    pub fn new(params: PingPongParams, sample_rate: f32) -> Self {
        let params = PingPongParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
        };
        Self {
            left_line: DelayLine::new(params.time_ms, sample_rate),
            right_line: DelayLine::new(params.time_ms, sample_rate),
            params,
        }
    }
}

impl StereoProcessor for PingPongDelay {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let [in_l, in_r] = input;
        let l_delayed = self.left_line.read();
        let r_delayed = self.right_line.read();
        // Cross-feedback: each input enters the OPPOSITE line, and each line's
        // output recirculates into the other — so the echo bounces L↔R.
        self.right_line
            .write(in_l + l_delayed * self.params.feedback);
        self.left_line
            .write(in_r + r_delayed * self.params.feedback);
        [
            mix_dry_wet(in_l, l_delayed, self.params.mix),
            mix_dry_wet(in_r, r_delayed, self.params.mix),
        ]
    }
}

pub fn build_stereo_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn StereoProcessor>> {
    Ok(Box::new(PingPongDelay::new(
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
    build_stereo_delay_processor(layout, || build_stereo_processor(params, sample_rate))
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
#[path = "native_ping_pong_tests.rs"]
mod tests;
