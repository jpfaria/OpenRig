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

pub const MODEL_ID: &str = "reverse";
pub const DISPLAY_NAME: &str = "Reverse Delay";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReverseDelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for ReverseDelayParams {
    fn default() -> Self {
        Self {
            time_ms: 500.0,
            feedback: 0.28,
            mix: 0.35,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Reverse Delay".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(ReverseDelayParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(ReverseDelayParams::default().feedback),
                0.0,
                MAX_FEEDBACK,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(ReverseDelayParams::default().mix),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<ReverseDelayParams> {
    Ok(ReverseDelayParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: required_f32(params, "feedback").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

pub struct ReverseDelay {
    params: ReverseDelayParams,
    sample_rate: f32,
    capture: Vec<f32>,
    playback: Vec<f32>,
    segment_len: usize,
    capture_pos: usize,
    playback_pos: usize,
    playback_len: usize,
}

impl ReverseDelay {
    pub fn new(params: ReverseDelayParams, sample_rate: f32) -> Self {
        let params = ReverseDelayParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
        };
        let max_len = (MAX_DELAY_MS * 0.001 * sample_rate).ceil() as usize + 2;
        let segment_len = segment_len_for(params.time_ms, sample_rate, max_len);
        Self {
            params,
            sample_rate,
            capture: vec![0.0; max_len],
            playback: vec![0.0; max_len],
            segment_len,
            capture_pos: 0,
            playback_pos: 0,
            playback_len: 0,
        }
    }

    fn refresh_segment_len(&mut self) {
        self.segment_len =
            segment_len_for(self.params.time_ms, self.sample_rate, self.capture.len());
    }
}

impl MonoProcessor for ReverseDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.refresh_segment_len();

        let delayed = if self.playback_pos < self.playback_len {
            let sample = self.playback[self.playback_pos];
            self.playback_pos += 1;
            sample
        } else {
            0.0
        };

        self.capture[self.capture_pos] = sanitize(input + delayed * self.params.feedback);
        self.capture_pos += 1;

        if self.capture_pos >= self.segment_len {
            for (dst, src) in self.playback[..self.segment_len]
                .iter_mut()
                .zip(self.capture[..self.segment_len].iter().rev())
            {
                *dst = *src;
            }
            self.playback_len = self.segment_len;
            self.playback_pos = 0;
            self.capture_pos = 0;
        }

        mix_dry_wet(input, delayed, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(ReverseDelay::new(
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
    brand: "",
    backend_kind: DelayBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

fn segment_len_for(time_ms: f32, sample_rate: f32, max_len: usize) -> usize {
    let raw = (clamp_time_ms(time_ms) * 0.001 * sample_rate).round() as usize;
    raw.clamp(8, max_len.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::MonoProcessor;

    #[test]
    fn reverse_delay_outputs_finite_values() {
        let mut delay = ReverseDelay::new(ReverseDelayParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }
}
