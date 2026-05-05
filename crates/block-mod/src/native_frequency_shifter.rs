//! Frequency shifter — single-sideband (SSB) modulation via the
//! Niemitalo IIR Hilbert pair. Pro-tier.
//!
//! Reference: Wardle, S. (1998). "A Hilbert-Transformer Frequency
//! Shifter for Audio." Proc. DAFx '98 Barcelona. Niemitalo, O. (1999).
//! "Polyphase IIR Hilbert Transformers" (yehar.com/blog/?p=368).
//!
//! Pro-tier vs. v1:
//!   * Replaces 63-tap FIR Hilbert with the Niemitalo 4-stage
//!     2nd-order all-pass pair → effective group delay drops from
//!     31 samples (~0.7 ms @ 44.1 k) to ~3 samples and the
//!     pre-ringing on transients goes away.
//!   * 8 multiplications per sample for the analytic-signal pair vs.
//!     ~63 for the FIR.
//!
//! The shift itself remains a complex multiply against `e^(j·2π·fs·t)`.
//! Real part of the product is the SSB output: every spectral
//! component moves by the same Hz amount (non-harmonic spectral skew,
//! the Bode-shifter signature).
//!
//! RT-safe: `HilbertIir` keeps 16 floats of state, no allocation.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::dsp::HilbertIir;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "frequency_shifter";
pub const DISPLAY_NAME: &str = "Frequency Shifter";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrequencyShifterParams {
    pub shift_hz: f32,
    pub mix: f32,
}

impl Default for FrequencyShifterParams {
    fn default() -> Self {
        Self {
            shift_hz: 50.0,
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
                "shift_hz",
                "Shift",
                None,
                Some(FrequencyShifterParams::default().shift_hz),
                -2_000.0,
                2_000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(FrequencyShifterParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<FrequencyShifterParams> {
    Ok(FrequencyShifterParams {
        shift_hz: required_f32(params, "shift_hz").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct FrequencyShifter {
    mix: f32,
    hilbert: HilbertIir,
    phase_inc: f32,
    phase: f32,
}

impl FrequencyShifter {
    pub fn new(shift_hz: f32, mix: f32, sample_rate: f32) -> Self {
        Self {
            mix: mix.clamp(0.0, 1.0),
            hilbert: HilbertIir::new(),
            phase_inc: TAU * shift_hz / sample_rate,
            phase: 0.0,
        }
    }
}

impl MonoProcessor for FrequencyShifter {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [real, imag] = self.hilbert.process(input);

        let (s, c) = self.phase.sin_cos();
        self.phase = (self.phase + self.phase_inc).rem_euclid(TAU);

        // y = Re{ (real + j*imag) * (cos + j*sin) } = real*cos - imag*sin
        let shifted = real * c - imag * s;

        (1.0 - self.mix) * real + self.mix * shifted
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(FrequencyShifter::new(p.shift_hz, p.mix, sample_rate)))
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
            struct StereoFreqShifter {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoFreqShifter {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            // Stereo: mirror shift sign on right for thru-zero spread
            // (Doepfer A-126 wiring).
            let p = params_from_set(params)?;
            Ok(block_core::BlockProcessor::Stereo(Box::new(
                StereoFreqShifter {
                    left: Box::new(FrequencyShifter::new(p.shift_hz, p.mix, sample_rate)),
                    right: Box::new(FrequencyShifter::new(-p.shift_hz, p.mix, sample_rate)),
                },
            )))
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
        let mut fs = FrequencyShifter::new(50.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = fs.process_sample(0.0);
            assert_eq!(out, 0.0, "shifter of silence must be silence");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut fs = FrequencyShifter::new(50.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = fs.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_real_path() {
        // mix=0 returns the H1 (real) leg directly.
        let mut fs = FrequencyShifter::new(50.0, 0.0, 44_100.0);
        let mut h_ref = HilbertIir::new();
        let sr = 44_100.0_f32;
        for i in 0..2048 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [expected_real, _] = h_ref.process(input);
            let out = fs.process_sample(input);
            assert!(
                (out - expected_real).abs() < 1e-6,
                "mix=0 should pass real leg unchanged at {i}: got {out} want {expected_real}"
            );
        }
    }

    #[test]
    fn zero_shift_close_to_real_path() {
        // shift=0 → e^(j·0) = 1 → wet = real. With mix=1, output =
        // real_leg, identical to the dry path with mix=0.
        let mut fs = FrequencyShifter::new(0.0, 1.0, 44_100.0);
        let mut h_ref = HilbertIir::new();
        let sr = 44_100.0_f32;
        for i in 0..2048 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [expected_real, _] = h_ref.process(input);
            let out = fs.process_sample(input);
            assert!(
                (out - expected_real).abs() < 1e-5,
                "shift=0 should ≈ real leg at {i}: got {out} want {expected_real}"
            );
        }
    }

    #[test]
    fn output_bounded_for_unit_input() {
        let mut fs = FrequencyShifter::new(100.0, 1.0, 44_100.0);
        for _ in 0..44_100 {
            let out = fs.process_sample(1.0);
            assert!(out.abs() < 5.0, "shifter output {out} too large");
        }
    }
}
