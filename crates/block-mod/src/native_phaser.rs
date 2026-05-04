//! Phaser — cascade of first-order all-pass filters with LFO-modulated
//! cutoff, summed (with feedback) into the dry signal.
//!
//! Reference: Smith, J. O. III. "Physical Audio Signal Processing"
//! (online book, CCRMA Stanford), section "Time-Varying Allpass
//! Filters." A first-order all-pass with break frequency `f` introduces
//! a frequency-dependent phase shift; cascading N stages and modulating
//! `f` with an LFO sweeps notches across the spectrum when summed with
//! the dry signal. Classic 4–6 stage analog phasers (MXR Phase 90, EHX
//! Small Stone) are the model.
//!
//! RT-safe: stack-allocated stage state, no allocation on the audio
//! thread.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::{PI, TAU};

pub const MODEL_ID: &str = "phaser_classic";
pub const DISPLAY_NAME: &str = "Classic Phaser";

const STAGES: usize = 6;
const MIN_FREQ_HZ: f32 = 200.0;
const MAX_FREQ_HZ: f32 = 2_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaserParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for PhaserParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.5,
            depth: 70.0,
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
                Some(PhaserParams::default().rate_hz),
                0.05,
                8.0,
                0.05,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(PhaserParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(PhaserParams::default().feedback),
                -95.0,
                95.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(PhaserParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<PhaserParams> {
    Ok(PhaserParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
        feedback: required_f32(params, "feedback").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct Phaser {
    rate_hz: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
    sample_rate: f32,
    phase: f32,
    stage_z: [f32; STAGES],
    feedback_z: f32,
}

impl Phaser {
    pub fn new(rate_hz: f32, depth: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            feedback: feedback.clamp(-0.95, 0.95),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
            stage_z: [0.0; STAGES],
            feedback_z: 0.0,
        }
    }
}

impl MonoProcessor for Phaser {
    fn process_sample(&mut self, input: f32) -> f32 {
        // LFO in [0,1]
        let lfo = 0.5 * (1.0 + self.phase.sin());
        self.phase = (self.phase + (TAU * self.rate_hz / self.sample_rate)).rem_euclid(TAU);

        // Sweep break frequency on a log scale so motion sounds even.
        let log_min = MIN_FREQ_HZ.ln();
        let log_max = MAX_FREQ_HZ.ln();
        let log_freq = log_min + (log_max - log_min) * self.depth * lfo;
        let break_hz = log_freq.exp();

        // First-order all-pass coefficient from break frequency:
        //   tan(pi * f / fs) - 1
        //   ─────────────────────
        //   tan(pi * f / fs) + 1
        let t = (PI * break_hz / self.sample_rate).tan();
        let a = (t - 1.0) / (t + 1.0);

        let mut x = input + self.feedback * self.feedback_z;
        for stage in self.stage_z.iter_mut() {
            // y[n] = a * x[n] + x[n-1] - a * y[n-1]
            // Using direct-form Transposed II in-place state update:
            let y = a * x + *stage;
            *stage = x - a * y;
            x = y;
        }
        self.feedback_z = x;

        (1.0 - self.mix) * input + self.mix * x
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(Phaser::new(
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
    let mut ph = Phaser::new(p.rate_hz, p.depth, p.feedback, p.mix, sample_rate);
    ph.phase = phase_offset;
    Ok(Box::new(ph))
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
            struct StereoPhaser {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoPhaser {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoPhaser {
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
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        for _ in 0..4096 {
            let out = ph.process_sample(0.0);
            assert_eq!(out, 0.0, "phaser of silence must be silence");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = ph.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = ph.process_sample(input);
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_with_clamped_feedback() {
        let mut ph = Phaser::new(0.5, 1.0, 1.5, 1.0, 44_100.0);
        for i in 0..44_100 {
            let input = ((i as f32 * 0.1).sin()).clamp(-1.0, 1.0);
            let out = ph.process_sample(input);
            assert!(out.abs() < 50.0, "diverged: {out} at {i}");
        }
    }
}
