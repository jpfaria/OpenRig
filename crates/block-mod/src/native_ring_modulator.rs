//! Ring modulator — `output = input * carrier_oscillator(carrier_freq)`.
//!
//! Reference: Bode, H. (1967). "A new tool for the exploration of audio
//! signals: Polyphonic frequency multiplier with low distortion."
//! AES Convention 32. The ring modulator is the canonical AM/DSB-SC
//! processor: a sine carrier multiplied by the input creates two
//! sidebands at `f_in ± f_carrier`, suppressing the original carrier
//! and input frequencies (hence "ring" — the diode-ring topology that
//! historically realized this).
//!
//! RT-safe: phase accumulator + sine table-free `sin()`. No allocation,
//! no lock, no syscall on the audio thread.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "ring_modulator";
pub const DISPLAY_NAME: &str = "Ring Modulator";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingModParams {
    pub carrier_hz: f32,
    pub mix: f32,
}

impl Default for RingModParams {
    fn default() -> Self {
        Self {
            carrier_hz: 220.0,
            mix: 100.0,
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
                "carrier_hz",
                "Carrier",
                None,
                Some(RingModParams::default().carrier_hz),
                20.0,
                4_000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(RingModParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<RingModParams> {
    Ok(RingModParams {
        carrier_hz: required_f32(params, "carrier_hz").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct RingModulator {
    carrier_hz: f32,
    mix: f32,
    sample_rate: f32,
    phase: f32,
}

impl RingModulator {
    pub fn new(carrier_hz: f32, mix: f32, sample_rate: f32) -> Self {
        Self {
            carrier_hz,
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
        }
    }
}

impl MonoProcessor for RingModulator {
    fn process_sample(&mut self, input: f32) -> f32 {
        let carrier = self.phase.sin();
        self.phase = (self.phase + (TAU * self.carrier_hz / self.sample_rate)).rem_euclid(TAU);
        let modulated = input * carrier;
        (1.0 - self.mix) * input + self.mix * modulated
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(RingModulator::new(p.carrier_hz, p.mix, sample_rate)))
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
            struct StereoRingMod {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoRingMod {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoRingMod {
                left: build_processor(params, sample_rate)?,
                right: build_processor(params, sample_rate)?,
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
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = rm.process_sample(0.0);
            assert_eq!(out, 0.0, "ring mod of silence must be silence");
        }
    }

    #[test]
    fn sine_input_output_finite_and_nonzero() {
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = rm.process_sample(input);
            assert!(out.is_finite(), "non-finite output at sample {i}");
            if out.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero ring-mod output for sine input");
    }

    #[test]
    fn output_bounded_by_input() {
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = rm.process_sample(1.0);
            assert!(out.abs() <= 1.0 + 1e-6, "ring-mod output {out} exceeds unit input");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut rm = RingModulator::new(220.0, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = rm.process_sample(input);
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry, got {out} expected {input}");
        }
    }
}
