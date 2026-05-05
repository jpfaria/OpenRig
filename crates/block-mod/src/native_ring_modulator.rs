//! Ring modulator — DSB-SC: `output = input × carrier`.
//!
//! Reference: Bode, H. (1967). "A new tool for the exploration of audio
//! signals: Polyphonic frequency multiplier with low distortion."
//! AES Convention 32. Multiplying input by a sine carrier yields two
//! sidebands at `f_in ± f_carrier`, suppressing both originals.
//!
//! Pro-tier topology:
//!   1. Upsample input to 2× rate (Oversampler2x, half-band FIR)
//!   2. Run carrier oscillator at 2× rate so its own spectrum stays
//!      below the up-rate Nyquist
//!   3. Multiply at 2× rate (sidebands now have headroom up to ~44 kHz
//!      before they alias)
//!   4. Downsample (anti-aliasing LP) back to base rate
//!   5. DC-block the wet (asymmetric input × asymmetric carrier can
//!      drift)
//!   6. Mix dry/wet
//!
//! RT-safe: Oversampler2x is stack-allocated, DcBlocker is one-pole.
//! Zero alloc, lock or syscall on the audio thread.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::dsp::{flush_denormal, DcBlocker, Oversampler2x};
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
    mix: f32,
    /// Carrier phase running at the up-rate (= 2 × sample_rate).
    phase: f32,
    /// Per-up-sample phase increment.
    phase_inc: f32,
    oversampler: Oversampler2x,
    dc_blocker: DcBlocker,
}

impl RingModulator {
    pub fn new(carrier_hz: f32, mix: f32, sample_rate: f32) -> Self {
        // Oversampler runs at 2× — phase increment uses 2 × sample_rate.
        let up_rate = 2.0 * sample_rate;
        Self {
            mix: mix.clamp(0.0, 1.0),
            phase: 0.0,
            phase_inc: TAU * carrier_hz / up_rate,
            oversampler: Oversampler2x::new(),
            // 5 Hz DC blocker — well below the audible band.
            dc_blocker: DcBlocker::new(5.0, sample_rate),
        }
    }
}

impl MonoProcessor for RingModulator {
    fn process_sample(&mut self, input: f32) -> f32 {
        // Step 1: upsample input to 2× rate.
        let [a, b] = self.oversampler.up(input);

        // Step 2: at 2× rate, modulate by carrier sample-by-sample.
        let mod_a = a * self.phase.sin();
        self.phase = (self.phase + self.phase_inc).rem_euclid(TAU);
        let mod_b = b * self.phase.sin();
        self.phase = (self.phase + self.phase_inc).rem_euclid(TAU);

        // Step 3: downsample back to base rate.
        let wet_raw = self.oversampler.down([mod_a, mod_b]);

        // Step 4: kill DC drift from asymmetric × asymmetric multiply.
        let wet = self.dc_blocker.process(flush_denormal(wet_raw));

        // Step 5: dry/wet mix.
        (1.0 - self.mix) * input + self.mix * wet
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
            // DC blocker may produce tiny denormal-flushed values
            // before reaching steady state, so allow a femto-tolerance.
            assert!(out.abs() < 1e-20, "ring mod of silence: {out}");
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
            assert!(out.is_finite(), "non-finite at {i}");
            if out.abs() > 1e-3 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero ring-mod output");
    }

    #[test]
    fn output_bounded_for_unit_input() {
        let mut rm = RingModulator::new(220.0, 1.0, 44_100.0);
        for _ in 0..2048 {
            let out = rm.process_sample(1.0);
            // Wet path passes through oversampler+DC blocker so peak
            // is bounded by ~|input| with small filter ringing
            // headroom.
            assert!(out.abs() < 1.5, "ring-mod output {out} out of bounds");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut rm = RingModulator::new(220.0, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = rm.process_sample(input);
            // mix=0 returns input directly (1-0)*input + 0*wet
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }
}
