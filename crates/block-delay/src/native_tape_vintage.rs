use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};

use crate::registry::{build_dual_mono_delay_processor, DelayModelDefinition};
use crate::DelayBackendKind;
use crate::shared::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, DelayLine, MAX_DELAY_MS,
    MAX_FEEDBACK, MIN_DELAY_MS,
};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "tape_vintage";
pub const DISPLAY_NAME: &str = "Tape Vintage Delay";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapeVintageParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub tone: f32,
    pub flutter: f32,
}

impl Default for TapeVintageParams {
    fn default() -> Self {
        Self {
            time_ms: 430.0,
            feedback: 42.0,
            mix: 32.0,
            tone: 42.0,
            flutter: 25.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "delay".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Tape Vintage Delay".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "time_ms",
                "Time",
                None,
                Some(TapeVintageParams::default().time_ms),
                MIN_DELAY_MS,
                MAX_DELAY_MS,
                1.0,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(TapeVintageParams::default().feedback),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(TapeVintageParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "tone",
                "Tone",
                None,
                Some(TapeVintageParams::default().tone),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "flutter",
                "Flutter",
                None,
                Some(TapeVintageParams::default().flutter),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<TapeVintageParams> {
    Ok(TapeVintageParams {
        time_ms: required_f32(params, "time_ms").map_err(Error::msg)?,
        feedback: {
            let value = required_f32(params, "feedback").map_err(Error::msg)?;
            (value / 100.0).min(MAX_FEEDBACK)
        },
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
        tone: required_f32(params, "tone").map_err(Error::msg)? / 100.0,
        flutter: required_f32(params, "flutter").map_err(Error::msg)? / 100.0,
    })
}

pub struct TapeVintageDelay {
    params: TapeVintageParams,
    line: DelayLine,
    tone_state: f32,
    wow_phase: f32,
    flutter_phase: f32,
}

impl TapeVintageDelay {
    pub fn new(params: TapeVintageParams, sample_rate: f32) -> Self {
        let params = TapeVintageParams {
            time_ms: clamp_time_ms(params.time_ms),
            feedback: clamp_feedback(params.feedback),
            mix: clamp_mix(params.mix),
            tone: params.tone.clamp(0.0, 1.0),
            flutter: params.flutter.clamp(0.0, 1.0),
        };
        Self {
            line: DelayLine::new(params.time_ms, sample_rate),
            params,
            tone_state: 0.0,
            wow_phase: 0.0,
            flutter_phase: 0.0,
        }
    }

    fn cutoff_hz(&self) -> f32 {
        let sample_rate = self.line.sample_rate();
        let min_cutoff = 380.0;
        let max_cutoff = (sample_rate * 0.3).min(6_500.0).max(min_cutoff);
        min_cutoff + (max_cutoff - min_cutoff) * self.params.tone
    }

    fn flutter_offset_ms(&mut self) -> f32 {
        let sample_rate = self.line.sample_rate();
        self.wow_phase = wrap_phase(self.wow_phase + TAU * 0.55 / sample_rate);
        self.flutter_phase = wrap_phase(self.flutter_phase + TAU * 4.7 / sample_rate);
        let modulation = self.wow_phase.sin() * 0.7 + self.flutter_phase.sin() * 0.3;
        modulation * self.params.flutter * 18.0
    }
}

impl MonoProcessor for TapeVintageDelay {
    fn process_sample(&mut self, input: f32) -> f32 {
        let modulated_time = self.params.time_ms + self.flutter_offset_ms();
        self.line.set_delay_ms(modulated_time);
        let delayed = self.line.read();
        let cutoff_hz = self.cutoff_hz();
        let sample_rate = self.line.sample_rate();
        let filtered = lowpass_step(&mut self.tone_state, delayed, cutoff_hz, sample_rate);
        self.line.write(input + filtered * self.params.feedback);
        mix_dry_wet(input, filtered, self.params.mix)
    }
}

pub fn build_mono_processor(
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<Box<dyn MonoProcessor>> {
    Ok(Box::new(TapeVintageDelay::new(
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

fn wrap_phase(phase: f32) -> f32 {
    if phase >= TAU {
        phase - TAU
    } else {
        phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_core::MonoProcessor;

    #[test]
    fn tape_vintage_outputs_finite_values() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 48_000.0);
        for _ in 0..10_000 {
            let output = delay.process_sample(0.2);
            assert!(output.is_finite());
        }
    }

    #[test]
    fn process_frame_silence_output_is_finite() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
        for i in 0..1024 {
            let out = delay.process_sample(0.0);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_frame_sine_output_is_finite() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
        for i in 0..1024 {
            let input = (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
            let out = delay.process_sample(input);
            assert!(out.is_finite(), "non-finite at sample {i}: {out}");
        }
    }

    #[test]
    fn process_block_1024_frames_all_finite() {
        let mut delay = TapeVintageDelay::new(TapeVintageParams::default(), 44100.0);
        let mut buf: Vec<f32> = (0..1024)
            .map(|i| (i as f32 / 44100.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        delay.process_block(&mut buf);
        for (i, &s) in buf.iter().enumerate() {
            assert!(s.is_finite(), "non-finite at index {i}: {s}");
        }
    }
}
