//! Modulated/Lush reverb — Jot 8-channel FDN with each delay line's
//! read tap modulated by an independent LFO. The micro pitch-shifts on
//! every line break up resonant modes and produce a shimmery, lush
//! tail commonly heard on Strymon Big Sky "Cloud" / Eventide Blackhole.
//!
//! References:
//! - Jot, J.-M. & Chaigne, A. "Digital Delay Networks for Designing
//!   Artificial Reverberators" (AES 90, 1991) — base FDN topology.
//! - Smith, Julius O. "Physical Audio Signal Processing", chapter on
//!   delay-line interpolation for modulation.
//!
//! Each line's delay tap is fractionally interpolated (linear) at
//! `base + sin(2π·f_i·t) · depth` samples behind the write head. Per-line
//! LFO frequencies are spread between 0.3 Hz and 1.7 Hz with co-prime-ish
//! ratios so the lines never share a phase relationship for long.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "modulated_lush";
pub const DISPLAY_NAME: &str = "Modulated / Lush";

const N: usize = 8;

const DELAY_MS: [f32; N] = [42.0, 47.0, 53.0, 59.0, 67.0, 73.0, 81.0, 89.0];
const LFO_HZ: [f32; N] = [0.31, 0.43, 0.57, 0.71, 0.89, 1.07, 1.31, 1.61];

const TAU: f32 = std::f32::consts::TAU;

struct Params {
    decay_pct: f32,
    damping: f32,
    mod_depth_ms: f32,    // peak excursion of each LFO in ms
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 70.0,
            damping: 25.0,
            mod_depth_ms: 1.5,
            mix: 30.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let d = Params::default();
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter("decay", "Decay", None, Some(d.decay_pct), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(d.damping), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mod_depth_ms", "Mod Depth", None, Some(d.mod_depth_ms), 0.0, 5.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        decay_pct: required_f32(params, "decay").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        mod_depth_ms: required_f32(params, "mod_depth_ms").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// Modulated delay line with linear-interpolated fractional read.
struct ModDelay {
    buf: Vec<f32>,
    write_idx: usize,
    base_samples: f32,
    depth_samples: f32,
    phase: f32,
    phase_inc: f32,
}

impl ModDelay {
    fn new(base_samples: f32, depth_samples: f32, lfo_hz: f32, sample_rate: f32) -> Self {
        // Allocate enough for the deepest possible read.
        let len = (base_samples + depth_samples + 4.0) as usize;
        Self {
            buf: vec![0.0; len.max(2)],
            write_idx: 0,
            base_samples,
            depth_samples,
            phase: 0.0,
            phase_inc: TAU * lfo_hz / sample_rate,
        }
    }

    fn read(&mut self) -> f32 {
        // LFO offset in samples (centred at base — so depth=0 → static delay).
        let offset = self.base_samples + self.depth_samples * self.phase.sin();
        self.phase += self.phase_inc;
        if self.phase > TAU {
            self.phase -= TAU;
        }

        let len = self.buf.len();
        let read_f = self.write_idx as f32 - offset;
        // Wrap into [0, len)
        let wrapped = read_f.rem_euclid(len as f32);
        let i0 = wrapped as usize;
        let i1 = (i0 + 1) % len;
        let frac = wrapped - i0 as f32;
        self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
    }

    fn write(&mut self, v: f32) {
        self.buf[self.write_idx] = v;
        self.write_idx = (self.write_idx + 1) % self.buf.len();
    }
}

struct OnePoleLpf { state: f32, coeff: f32 }
impl OnePoleLpf {
    fn new() -> Self { Self { state: 0.0, coeff: 0.0 } }
    fn set_damping(&mut self, d: f32) { self.coeff = d.clamp(0.0, 1.0).sqrt(); }
    fn process(&mut self, x: f32) -> f32 {
        self.state = (1.0 - self.coeff).mul_add(x, self.coeff * self.state);
        self.state
    }
}

fn hadamard8(x: &mut [f32; N]) {
    for i in (0..N).step_by(2) {
        let a = x[i]; let b = x[i + 1];
        x[i] = a + b; x[i + 1] = a - b;
    }
    for base in (0..N).step_by(4) {
        let a0 = x[base]; let a1 = x[base + 1];
        let a2 = x[base + 2]; let a3 = x[base + 3];
        x[base] = a0 + a2; x[base + 1] = a1 + a3;
        x[base + 2] = a0 - a2; x[base + 3] = a1 - a3;
    }
    for i in 0..4 {
        let a = x[i]; let b = x[i + 4];
        x[i] = a + b; x[i + 4] = a - b;
    }
    let inv = 1.0 / (N as f32).sqrt();
    for v in x.iter_mut() { *v *= inv; }
}

struct LushReverb {
    params: Params,
    delays: [ModDelay; N],
    lpfs: [OnePoleLpf; N],
    feedback: f32,
}

impl LushReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let feedback = (0.6 + params.decay_pct * 0.37).clamp(0.0, 0.97);
        let depth_samples = (params.mod_depth_ms / 1000.0) * sample_rate;

        let delays: [ModDelay; N] = std::array::from_fn(|i| {
            let base = (DELAY_MS[i] / 1000.0) * sample_rate;
            ModDelay::new(base, depth_samples, LFO_HZ[i], sample_rate)
        });
        let lpfs: [OnePoleLpf; N] = std::array::from_fn(|_| {
            let mut f = OnePoleLpf::new();
            f.set_damping(params.damping);
            f
        });

        Self { params, delays, lpfs, feedback }
    }
}

impl StereoProcessor for LushReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let in_mono = (input[0] + input[1]) * 0.5;

        let mut s: [f32; N] = std::array::from_fn(|i| self.delays[i].read());

        let wet_l = (s[0] + s[2] + s[4] + s[6]) * 0.25;
        let wet_r = (s[1] + s[3] + s[5] + s[7]) * 0.25;

        for i in 0..N {
            s[i] = self.lpfs[i].process(s[i]) * self.feedback;
        }
        hadamard8(&mut s);
        for i in 0..N {
            self.delays[i].write(s[i] + in_mono * 0.25);
        }

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct LushAsMono(LushReverb);

impl MonoProcessor for LushAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [left, _] = StereoProcessor::process_frame(&mut self.0, [input, input]);
        left
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = params_from_set(params)?;
    match layout {
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(LushReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(LushAsMono(LushReverb::new(p, sample_rate))))),
    }
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: ReverbBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn default_reverb() -> LushReverb {
        LushReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn impulse_response_finite() {
        let mut reverb = default_reverb();
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn silence_input_produces_finite_silence() {
        let mut reverb = default_reverb();
        for i in 0..2048 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn sine_input_produces_finite_nonzero_output() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..4096 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero);
    }

    #[test]
    fn mod_delay_with_zero_depth_acts_as_static_delay() {
        let sr = 44_100.0_f32;
        let mut d = ModDelay::new(100.0, 0.0, 1.0, sr);
        // Write 200 samples then read — should reproduce the input N samples ago.
        for i in 0..200 {
            let s = i as f32 * 0.01;
            // Read first to populate the LFO advance, then write (matches the
            // ordering used inside StereoProcessor::process_frame).
            let _ = d.read();
            d.write(s);
        }
        // Now read — should be approximately sample (current_write - 100).
        // Since we just wrote 200 samples, current write_idx points at 200%len.
        // Sample at offset 100 back is what we wrote at index 100, value 0.01 * 100 = 1.0.
        let out = d.read();
        assert!((out - 1.0).abs() < 0.01, "expected ~1.0, got {out}");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = LushAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..4096 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
}
