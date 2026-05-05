//! Shimmer reverb — Eventide H8000 / Strymon Big Sky "Shimmer" character.
//! An FDN tail with the wet output pitch-shifted up an octave and fed
//! back into the input. Each pass adds another stacked octave of the
//! original signal, producing the recursive ethereal sound.
//!
//! References:
//! - Jot, J.-M. & Chaigne, A. "Digital Delay Networks for Designing
//!   Artificial Reverberators" (AES 90, 1991) — base FDN topology.
//! - Bernsee, S. M. (2003). "Pitch shifting using the Fourier transform".
//!   We use a simpler time-domain SOLA-style shifter with two crossfaded
//!   read pointers — adequate for the +1 octave fixed ratio (no need for
//!   FFT/phase vocoder when the shift is constant and integer-octave).
//!
//! Time-domain shifter: a circular buffer of length L is written at 1x
//! speed and read by two pointers advancing at 2x speed (for +12 semi).
//! The two pointers are offset by L/2 and each is multiplied by a
//! Hann-style window centred on its own range, so the sum of the two
//! windows is approximately constant (no amplitude modulation).

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "shimmer";
pub const DISPLAY_NAME: &str = "Shimmer";

const N: usize = 8;

const DELAY_MS: [f32; N] = [42.0, 47.0, 53.0, 59.0, 67.0, 73.0, 81.0, 89.0];

struct Params {
    decay_pct: f32,
    damping: f32,
    shimmer_amount: f32,  // 0..1 — how much of the wet feedback is pitch-shifted
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 70.0,
            damping: 25.0,
            shimmer_amount: 50.0,
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
            float_parameter("shimmer_amount", "Shimmer", None, Some(d.shimmer_amount), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        decay_pct: required_f32(params, "decay").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        shimmer_amount: required_f32(params, "shimmer_amount").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// Time-domain octave-up pitch shifter (fixed 2x ratio) using two
/// crossfaded read pointers in a circular buffer. Constant-power Hann
/// windows summing to ~1 across both reads.
struct OctaveUp {
    buf: Vec<f32>,
    write_idx: usize,
    read1_pos: f32,
    read2_pos: f32,
    window_len: f32,
}

impl OctaveUp {
    fn new(window_samples: usize) -> Self {
        // Buffer holds at least 2x the window so reads at 2x speed never
        // overrun the write head within one window cycle.
        let buf_len = (window_samples * 2 + 4).max(64);
        Self {
            buf: vec![0.0; buf_len],
            write_idx: 0,
            read1_pos: 0.0,
            read2_pos: window_samples as f32 / 2.0,
            window_len: window_samples as f32,
        }
    }

    fn step(&mut self, input: f32) -> f32 {
        // Write current sample.
        self.buf[self.write_idx] = input;

        let len = self.buf.len();
        let win = self.window_len;

        // Read at 2x speed for +12 semitones.
        let read = |pos: f32| -> f32 {
            let p = pos.rem_euclid(len as f32);
            let i0 = p as usize;
            let i1 = (i0 + 1) % len;
            let frac = p - i0 as f32;
            self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
        };
        let win_at = |pos: f32| -> f32 {
            // Hann window centred on pos's window phase: position within window.
            let phase = (self.write_idx as f32 - pos).rem_euclid(win) / win;
            // 0..1 → 0..π → 0..1..0
            0.5 * (1.0 - (std::f32::consts::TAU * phase).cos())
        };

        let r1 = read(self.read1_pos) * win_at(self.read1_pos);
        let r2 = read(self.read2_pos) * win_at(self.read2_pos);
        let out = r1 + r2;

        // Advance read pointers at 2x for octave up.
        self.read1_pos += 2.0;
        self.read2_pos += 2.0;
        // Wrap into buffer range to avoid f32 precision loss over time.
        if self.read1_pos >= len as f32 {
            self.read1_pos -= len as f32;
        }
        if self.read2_pos >= len as f32 {
            self.read2_pos -= len as f32;
        }

        self.write_idx = (self.write_idx + 1) % len;
        out
    }
}

struct DelayLine {
    buf: Vec<f32>,
    write_idx: usize,
}
impl DelayLine {
    fn new(samples: usize) -> Self {
        Self { buf: vec![0.0; samples.max(1)], write_idx: 0 }
    }
    fn read(&self) -> f32 { self.buf[self.write_idx] }
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

struct ShimmerReverb {
    params: Params,
    delays: [DelayLine; N],
    lpfs: [OnePoleLpf; N],
    feedback: f32,
    pitch_l: OctaveUp,
    pitch_r: OctaveUp,
}

impl ShimmerReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let feedback = (0.6 + params.decay_pct * 0.35).clamp(0.0, 0.95);

        let delays: [DelayLine; N] = std::array::from_fn(|i| {
            let s = ((DELAY_MS[i] / 1000.0) * sample_rate) as usize;
            DelayLine::new(s)
        });
        let lpfs: [OnePoleLpf; N] = std::array::from_fn(|_| {
            let mut f = OnePoleLpf::new();
            f.set_damping(params.damping);
            f
        });

        // 50ms pitch-shift window — short enough to not smear transients,
        // long enough that octave-up artifacts stay below the noise floor.
        let pitch_window = (sample_rate * 0.05) as usize;
        let pitch_l = OctaveUp::new(pitch_window);
        let pitch_r = OctaveUp::new(pitch_window);

        Self { params, delays, lpfs, feedback, pitch_l, pitch_r }
    }
}

impl StereoProcessor for ShimmerReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let in_mono = (input[0] + input[1]) * 0.5;

        let mut s: [f32; N] = std::array::from_fn(|i| self.delays[i].read());

        let wet_l = (s[0] + s[2] + s[4] + s[6]) * 0.25;
        let wet_r = (s[1] + s[3] + s[5] + s[7]) * 0.25;

        // Pitch-shift the wet output up an octave for the shimmer feedback.
        let shimmer_l = self.pitch_l.step(wet_l) * self.params.shimmer_amount;
        let shimmer_r = self.pitch_r.step(wet_r) * self.params.shimmer_amount;
        let shimmer_mono = (shimmer_l + shimmer_r) * 0.5;

        for i in 0..N {
            s[i] = self.lpfs[i].process(s[i]) * self.feedback;
        }
        hadamard8(&mut s);

        for i in 0..N {
            // Inject both the dry input AND the shimmer feedback.
            self.delays[i].write(s[i] + (in_mono + shimmer_mono) * 0.25);
        }

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct ShimmerAsMono(ShimmerReverb);

impl MonoProcessor for ShimmerAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(ShimmerReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(ShimmerAsMono(ShimmerReverb::new(p, sample_rate))))),
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

    fn default_reverb() -> ShimmerReverb {
        ShimmerReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn octave_up_silence_in_silence_out() {
        let mut p = OctaveUp::new(2048);
        for _ in 0..8192 {
            assert!(p.step(0.0).abs() < 1e-6);
        }
    }

    #[test]
    fn octave_up_finite_for_sine_input() {
        let mut p = OctaveUp::new(2048);
        let sr = 44_100.0_f32;
        for i in 0..8192 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            assert!(p.step(s).is_finite());
        }
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
        for i in 0..8192 {
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
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = ShimmerAsMono(default_reverb());
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
