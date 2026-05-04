//! Flanger — short modulated delay line with feedback.
//!
//! Reference: Dattorro, J. (1997). "Effect Design Part 2: Delay-Line
//! Modulation and Chorus." Journal of the Audio Engineering Society,
//! 45(10), pp. 764-788. The flanger is a delay-line in the 0.5–10 ms
//! range, length-modulated by a low-frequency oscillator, summed
//! (optionally with feedback) into the dry signal. Comb-filter notches
//! sweep across the spectrum producing the characteristic "jet" sound.
//!
//! RT-safe: pre-allocated ring buffer, fractional read via linear
//! interpolation. No allocation, lock, syscall on the audio thread.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "flanger_classic";
pub const DISPLAY_NAME: &str = "Classic Flanger";

const MAX_DELAY_MS: f32 = 12.0;
const BASE_DELAY_MS: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlangerParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for FlangerParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.4,
            depth: 60.0,
            feedback: 50.0,
            mix: 50.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(FlangerParams::default().rate_hz),
                0.05,
                5.0,
                0.05,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(FlangerParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(FlangerParams::default().feedback),
                -95.0,
                95.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(FlangerParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<FlangerParams> {
    Ok(FlangerParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
        feedback: required_f32(params, "feedback").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct Flanger {
    rate_hz: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
    sample_rate: f32,
    buffer: Vec<f32>,
    write_idx: usize,
    phase: f32,
    base_samples: f32,
    sweep_samples: f32,
}

impl Flanger {
    pub fn new(rate_hz: f32, depth: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        let max_samples = ((MAX_DELAY_MS / 1000.0) * sample_rate).ceil() as usize + 4;
        let base_samples = (BASE_DELAY_MS / 1000.0) * sample_rate;
        let sweep_samples = ((MAX_DELAY_MS - BASE_DELAY_MS) / 1000.0) * sample_rate;
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            feedback: feedback.clamp(-0.95, 0.95),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            buffer: vec![0.0; max_samples],
            write_idx: 0,
            phase: 0.0,
            base_samples,
            sweep_samples,
        }
    }

    fn read_interp(&self, delay_samples: f32) -> f32 {
        let len = self.buffer.len();
        let read_pos = self.write_idx as f32 - delay_samples;
        let read_pos = read_pos.rem_euclid(len as f32);
        let i0 = read_pos.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = read_pos - read_pos.floor();
        self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac
    }
}

impl MonoProcessor for Flanger {
    fn process_sample(&mut self, input: f32) -> f32 {
        // LFO in [0,1]
        let lfo = 0.5 * (1.0 + self.phase.sin());
        self.phase = (self.phase + (TAU * self.rate_hz / self.sample_rate)).rem_euclid(TAU);

        let delay = self.base_samples + self.sweep_samples * self.depth * lfo;
        let delayed = self.read_interp(delay);

        let to_buffer = input + self.feedback * delayed;
        self.buffer[self.write_idx] = to_buffer;
        self.write_idx = (self.write_idx + 1) % self.buffer.len();

        (1.0 - self.mix) * input + self.mix * delayed
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(Flanger::new(
        p.rate_hz,
        p.depth,
        p.feedback,
        p.mix,
        sample_rate,
    )))
}

pub fn build_processor_with_phase(
    params: &ParameterSet,
    sample_rate: f32,
    phase_offset: f32,
) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    let mut f = Flanger::new(p.rate_hz, p.depth, p.feedback, p.mix, sample_rate);
    f.phase = phase_offset;
    Ok(Box::new(f))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    match layout {
        block_core::AudioChannelLayout::Mono => Ok(block_core::BlockProcessor::Mono(
            build_processor(params, sample_rate)?,
        )),
        block_core::AudioChannelLayout::Stereo => {
            struct StereoFlanger {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoFlanger {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoFlanger {
                left: build_processor(params, sample_rate)?,
                right: build_processor_with_phase(params, sample_rate, std::f32::consts::PI)?,
            })))
        }
    }
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: ModBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_in_silence_out() {
        let mut f = Flanger::new(0.4, 0.6, 0.5, 0.5, 44_100.0);
        for _ in 0..4096 {
            let out = f.process_sample(0.0);
            assert_eq!(out, 0.0, "flanger of silence must be silence");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut f = Flanger::new(0.4, 0.6, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = f.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut f = Flanger::new(0.4, 0.6, 0.5, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = f.process_sample(input);
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_with_clamped_feedback() {
        // With clamped feedback (|fb| ≤ 0.95) and bounded input,
        // delay-line cannot diverge.
        let mut f = Flanger::new(0.4, 1.0, 1.5, 1.0, 44_100.0); // 1.5 -> clamped to 0.95
        for i in 0..44_100 {
            let input = ((i as f32 * 0.1).sin()).clamp(-1.0, 1.0);
            let out = f.process_sample(input);
            assert!(out.abs() < 50.0, "diverged: {out} at {i}");
        }
    }
}
