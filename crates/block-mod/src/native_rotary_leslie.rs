//! Rotary speaker (Leslie) — dual-rotor model.
//!
//! Reference: Smith, J. O. III. "Rotary Speaker (Leslie)" tutorial in
//! "Physical Audio Signal Processing" (CCRMA online); Henricksen, C.
//! "Unearthing the Mysteries of the Leslie Cabinet" Recording
//! Engineer/Producer 1981. Topology:
//!
//!   1. Linkwitz-Riley 4th-order crossover at 800 Hz splits the input
//!      into a high-band (horn) and low-band (drum / bass rotor).
//!   2. Each rotor is modulated by an LFO at its own rate:
//!         * Doppler delay-line modulation (~ pitch wobble)
//!         * Amplitude modulation (tremolo from rotation past the mics)
//!         * Stereo pan as the rotor faces away from L vs R mic
//!   3. The two rotor outputs are summed stereo.
//!
//! Two speeds: SLOW ("chorale") and FAST ("tremolo"). Real Leslie 122
//! rotors:
//!         horn    drum
//!   slow  0.83 Hz 0.67 Hz
//!   fast  6.40 Hz 5.70 Hz
//!
//! RT-safe: pre-allocated delay rings for both rotors; no allocation,
//! lock or syscall on the audio thread.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::{PI, TAU};

pub const MODEL_ID: &str = "rotary_leslie";
pub const DISPLAY_NAME: &str = "Rotary Leslie";

const CROSSOVER_HZ: f32 = 800.0;

const HORN_DELAY_BASE_MS: f32 = 0.7;
const HORN_DELAY_DEPTH_MS: f32 = 0.4;
const DRUM_DELAY_BASE_MS: f32 = 1.5;
const DRUM_DELAY_DEPTH_MS: f32 = 0.9;

const HORN_AM_DEPTH: f32 = 0.35;
const DRUM_AM_DEPTH: f32 = 0.20;

const HORN_RATE_SLOW_HZ: f32 = 0.83;
const HORN_RATE_FAST_HZ: f32 = 6.40;
const DRUM_RATE_SLOW_HZ: f32 = 0.67;
const DRUM_RATE_FAST_HZ: f32 = 5.70;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeslieParams {
    /// 0.0 = slow (chorale), 1.0 = fast (tremolo). Continuous so that
    /// transitions between the two speeds spin up/down naturally as a
    /// real Leslie does.
    pub speed: f32,
    pub mix: f32,
}

impl Default for LeslieParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
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
                "speed",
                "Speed",
                None,
                Some(LeslieParams::default().speed * 100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(LeslieParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<LeslieParams> {
    Ok(LeslieParams {
        speed: required_f32(params, "speed").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// RBJ cookbook biquad — direct-form I, single-channel.
#[derive(Default, Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn lowpass(cutoff_hz: f32, sample_rate: f32, q: f32) -> Self {
        let w0 = TAU * cutoff_hz / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn highpass(cutoff_hz: f32, sample_rate: f32, q: f32) -> Self {
        let w0 = TAU * cutoff_hz / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

struct Rotor {
    sample_rate: f32,
    base_delay_samples: f32,
    depth_samples: f32,
    am_depth: f32,
    buffer: Vec<f32>,
    write_idx: usize,
    phase: f32,
    rate_hz: f32,
}

impl Rotor {
    fn new(
        sample_rate: f32,
        base_delay_ms: f32,
        depth_ms: f32,
        am_depth: f32,
        rate_hz: f32,
        phase_offset: f32,
    ) -> Self {
        let max_samples =
            (((base_delay_ms + depth_ms) / 1000.0) * sample_rate).ceil() as usize + 8;
        Self {
            sample_rate,
            base_delay_samples: (base_delay_ms / 1000.0) * sample_rate,
            depth_samples: (depth_ms / 1000.0) * sample_rate,
            am_depth: am_depth.clamp(0.0, 1.0),
            buffer: vec![0.0; max_samples],
            write_idx: 0,
            phase: phase_offset,
            rate_hz,
        }
    }

    fn read_interp(&self, delay_samples: f32) -> f32 {
        let len = self.buffer.len();
        let pos = self.write_idx as f32 - delay_samples;
        let pos = pos.rem_euclid(len as f32);
        let i0 = pos.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = pos - pos.floor();
        self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac
    }

    fn step(&mut self, input: f32, target_rate_hz: f32) -> [f32; 2] {
        // Single-pole smoothing on rate to spin-up/down like a motor.
        // tau ~= 0.5s independent of sample rate.
        let tau_samples = 0.5 * self.sample_rate;
        let alpha = 1.0 / tau_samples;
        self.rate_hz += (target_rate_hz - self.rate_hz) * alpha;

        let lfo_sin = self.phase.sin();
        self.phase = (self.phase + (TAU * self.rate_hz / self.sample_rate)).rem_euclid(TAU);

        // Doppler: instantaneous delay sweeps base ± depth.
        let delay = self.base_delay_samples + self.depth_samples * lfo_sin;
        // Write input.
        self.buffer[self.write_idx] = input;
        self.write_idx = (self.write_idx + 1) % self.buffer.len();
        let delayed = self.read_interp(delay);

        // AM tremolo (rotor pointing toward a mic = louder).
        let am_l = 1.0 - self.am_depth * (1.0 + lfo_sin) * 0.5;
        let am_r = 1.0 - self.am_depth * (1.0 - lfo_sin) * 0.5;

        [delayed * am_l, delayed * am_r]
    }
}

pub struct LeslieRotary {
    speed: f32,
    mix: f32,
    crossover_lp_a: Biquad,
    crossover_lp_b: Biquad,
    crossover_hp_a: Biquad,
    crossover_hp_b: Biquad,
    horn: Rotor,
    drum: Rotor,
}

impl LeslieRotary {
    pub fn new(speed: f32, mix: f32, sample_rate: f32) -> Self {
        // LR4 = two cascaded Butterworth 2nd-order (Q = 1/sqrt(2)).
        let q = 1.0 / 2.0_f32.sqrt();
        Self {
            speed: speed.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            crossover_lp_a: Biquad::lowpass(CROSSOVER_HZ, sample_rate, q),
            crossover_lp_b: Biquad::lowpass(CROSSOVER_HZ, sample_rate, q),
            crossover_hp_a: Biquad::highpass(CROSSOVER_HZ, sample_rate, q),
            crossover_hp_b: Biquad::highpass(CROSSOVER_HZ, sample_rate, q),
            horn: Rotor::new(
                sample_rate,
                HORN_DELAY_BASE_MS,
                HORN_DELAY_DEPTH_MS,
                HORN_AM_DEPTH,
                HORN_RATE_SLOW_HZ,
                0.0,
            ),
            drum: Rotor::new(
                sample_rate,
                DRUM_DELAY_BASE_MS,
                DRUM_DELAY_DEPTH_MS,
                DRUM_AM_DEPTH,
                DRUM_RATE_SLOW_HZ,
                PI, // start drum 180° out so onset doesn't pile
            ),
        }
    }

    fn target_rates(&self) -> (f32, f32) {
        let s = self.speed;
        (
            HORN_RATE_SLOW_HZ + s * (HORN_RATE_FAST_HZ - HORN_RATE_SLOW_HZ),
            DRUM_RATE_SLOW_HZ + s * (DRUM_RATE_FAST_HZ - DRUM_RATE_SLOW_HZ),
        )
    }

    pub fn process_stereo(&mut self, dry_in: f32) -> [f32; 2] {
        let lo = self.crossover_lp_b.process(self.crossover_lp_a.process(dry_in));
        let hi = self.crossover_hp_b.process(self.crossover_hp_a.process(dry_in));

        let (horn_rate, drum_rate) = self.target_rates();
        let [horn_l, horn_r] = self.horn.step(hi, horn_rate);
        let [drum_l, drum_r] = self.drum.step(lo, drum_rate);

        let wet_l = horn_l + drum_l;
        let wet_r = horn_r + drum_r;

        [
            (1.0 - self.mix) * dry_in + self.mix * wet_l,
            (1.0 - self.mix) * dry_in + self.mix * wet_r,
        ]
    }
}

/// Mono-output adapter for the layout=Mono path: sums L+R of the
/// stereo Leslie engine (-3 dB so a centered tone keeps unity).
pub struct LeslieMono {
    inner: LeslieRotary,
}

impl MonoProcessor for LeslieMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = self.inner.process_stereo(input);
        0.5 * (l + r)
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    let p = params_from_set(params)?;
    match layout {
        block_core::AudioChannelLayout::Mono => Ok(block_core::BlockProcessor::Mono(Box::new(
            LeslieMono {
                inner: LeslieRotary::new(p.speed, p.mix, sample_rate),
            },
        ))),
        block_core::AudioChannelLayout::Stereo => {
            struct LeslieStereoProc {
                inner: LeslieRotary,
            }

            impl block_core::StereoProcessor for LeslieStereoProc {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    // Drive the Leslie with the L+R sum (Leslie is mono-in,
                    // stereo-out by topology — two mics around the cabinet).
                    let mono_in = 0.5 * (input[0] + input[1]);
                    self.inner.process_stereo(mono_in)
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(
                LeslieStereoProc {
                    inner: LeslieRotary::new(p.speed, p.mix, sample_rate),
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
        let mut l = LeslieRotary::new(1.0, 1.0, 44_100.0);
        for _ in 0..8192 {
            let [a, b] = l.process_stereo(0.0);
            assert_eq!(a, 0.0);
            assert_eq!(b, 0.0);
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut l = LeslieRotary::new(1.0, 1.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..8192 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [a, b] = l.process_stereo(input);
            assert!(a.is_finite() && b.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut l = LeslieRotary::new(1.0, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let [a, b] = l.process_stereo(input);
            assert!((a - input).abs() < 1e-6, "L mix=0 should be dry");
            assert!((b - input).abs() < 1e-6, "R mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_for_unit_input() {
        let mut l = LeslieRotary::new(1.0, 1.0, 44_100.0);
        for _ in 0..44_100 {
            let [a, b] = l.process_stereo(1.0);
            assert!(a.abs() < 5.0 && b.abs() < 5.0, "rotary output too large");
        }
    }
}
