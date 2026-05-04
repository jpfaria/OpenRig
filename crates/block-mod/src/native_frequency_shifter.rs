//! Frequency shifter — single-sideband (SSB) modulation via Hilbert
//! transform.
//!
//! Reference: Wardle, S. (1998). "A Hilbert-Transformer Frequency
//! Shifter for Audio." Proc. DAFx '98, Barcelona. The shifter forms
//! the analytic signal `z(t) = x(t) + j·H{x}(t)`, multiplies by a
//! complex carrier `e^(j·2π·fs·t)`, and takes the real part — yielding
//! a non-harmonic frequency shift (every spectral component moves by
//! the same Hz amount, unlike a pitch-shifter which preserves
//! intervals). The Bode shifter circuit is the analog forerunner.
//!
//! Implementation: 63-tap Type III FIR Hilbert with Hamming window
//! (anti-symmetric, odd-length). The dry path is delayed by 31 samples
//! to align with the Hilbert group-delay; the analytic-signal halves
//! are then complex-mixed with a sine/cosine pair driven by the shift
//! frequency.
//!
//! RT-safe: 64-sample ring buffer + 63 pre-computed FIR taps + sin/cos
//! per sample. No allocation, lock, or syscall on the audio thread.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::{PI, TAU};

pub const MODEL_ID: &str = "frequency_shifter";
pub const DISPLAY_NAME: &str = "Frequency Shifter";

const FIR_LEN: usize = 63;
const FIR_CENTER: usize = FIR_LEN / 2; // 31
const BUFFER_LEN: usize = 64; // power of 2 mask

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

/// Build Hamming-windowed Type III FIR Hilbert transformer.
/// h[n] = (2 / (pi * d)) * w[n] for odd d, zero for even d (incl. center).
fn build_hilbert_taps() -> [f32; FIR_LEN] {
    let mut taps = [0.0_f32; FIR_LEN];
    let center = FIR_CENTER as i32;
    for n in 0..FIR_LEN {
        let d = n as i32 - center;
        let coef = if d == 0 || d % 2 == 0 {
            0.0
        } else {
            2.0 / (PI * d as f32)
        };
        // Hamming window
        let w = 0.54 - 0.46 * ((TAU * n as f32) / (FIR_LEN as f32 - 1.0)).cos();
        taps[n] = coef * w;
    }
    taps
}

pub struct FrequencyShifter {
    shift_hz: f32,
    mix: f32,
    sample_rate: f32,
    taps: [f32; FIR_LEN],
    ring: [f32; BUFFER_LEN],
    write_idx: usize,
    phase: f32,
}

impl FrequencyShifter {
    pub fn new(shift_hz: f32, mix: f32, sample_rate: f32) -> Self {
        Self {
            shift_hz,
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            taps: build_hilbert_taps(),
            ring: [0.0; BUFFER_LEN],
            write_idx: 0,
            phase: 0.0,
        }
    }

    fn read(&self, delay: usize) -> f32 {
        let idx = (self.write_idx + BUFFER_LEN - delay) % BUFFER_LEN;
        self.ring[idx]
    }
}

impl MonoProcessor for FrequencyShifter {
    fn process_sample(&mut self, input: f32) -> f32 {
        // Write current input to ring.
        self.ring[self.write_idx] = input;
        self.write_idx = (self.write_idx + 1) % BUFFER_LEN;

        // Hilbert (imag part of analytic signal).
        let mut imag = 0.0_f32;
        for k in 0..FIR_LEN {
            // taps[k] convolves with x[n-k]; we want index relative to
            // most recent sample (just written). Most recent is at
            // write_idx-1, so x[n-k] is at write_idx-1-k.
            imag += self.taps[k] * self.read(k + 1);
        }
        // Real part (delayed input to align with Hilbert group delay).
        let real = self.read(FIR_CENTER + 1);

        // Complex multiply by e^(j*phi).
        let (s, c) = self.phase.sin_cos();
        self.phase = (self.phase + (TAU * self.shift_hz / self.sample_rate)).rem_euclid(TAU);

        // y = Re{ (real + j*imag) * (c + j*s) } = real*c - imag*s
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

            // Stereo: invert shift sign on right for "thru-zero" stereo
            // spread effect (mirrors Bode/Doepfer A-126 stereo wiring).
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
    fn dry_mix_passes_delayed_input() {
        // mix=0 returns the delayed real path (Hilbert group delay).
        let mut fs = FrequencyShifter::new(50.0, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        let inputs: Vec<f32> = (0..4096)
            .map(|i| (TAU * 440.0 * i as f32 / sr).sin())
            .collect();
        let outs: Vec<f32> = inputs.iter().map(|&x| fs.process_sample(x)).collect();
        // After warmup, output equals input shifted by FIR_CENTER+1 samples.
        for i in (FIR_CENTER + 64)..outs.len() {
            let expected = inputs[i - (FIR_CENTER + 1)];
            assert!(
                (outs[i] - expected).abs() < 1e-5,
                "mix=0 should be delayed dry: got {} expected {} at {i}",
                outs[i],
                expected
            );
        }
    }

    #[test]
    fn zero_shift_is_close_to_dry() {
        // With shift=0 the analytic signal is multiplied by 1+0j, so
        // output should match the delayed dry path.
        let mut fs = FrequencyShifter::new(0.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        let inputs: Vec<f32> = (0..4096)
            .map(|i| (TAU * 440.0 * i as f32 / sr).sin())
            .collect();
        let outs: Vec<f32> = inputs.iter().map(|&x| fs.process_sample(x)).collect();
        for i in (FIR_CENTER + 64)..outs.len() {
            let expected = inputs[i - (FIR_CENTER + 1)];
            assert!(
                (outs[i] - expected).abs() < 1e-4,
                "shift=0 should be ~dry: got {} expected {} at {i}",
                outs[i],
                expected
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
