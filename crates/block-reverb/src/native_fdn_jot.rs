//! Feedback Delay Network reverb (Jot 1991 / Jot & Chaigne 1991) — 8-channel
//! variant with a Walsh-Hadamard mixing matrix and per-line lowpass damping.
//!
//! References:
//! - Jot, Jean-Marc. "An Analysis/Synthesis Approach to Real-Time Artificial
//!   Reverberation" (ICASSP 1992).
//! - Jot, J.-M. & Chaigne, A. "Digital Delay Networks for Designing
//!   Artificial Reverberators" (AES 90, 1991).
//!
//! Compared to a comb+allpass network (Schroeder/Moorer/Freeverb) the FDN
//! produces a denser, modally smoother tail because every delay output is
//! recirculated through *every other* delay's input via the mixing matrix.
//! 8 lines is the classic "Jot demo" sweet spot — denser than 4, much
//! cheaper than 16/32 for similar perceptual density on guitar.
//!
//! The 8x8 Walsh-Hadamard matrix is unitary (energy-preserving) and
//! computes via a butterfly in O(N log N) instead of O(N^2).

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "fdn_jot";
pub const DISPLAY_NAME: &str = "FDN Reverb (Jot)";

const N: usize = 8;

// Co-prime-ish delay lengths in ms — chosen to spread modal density and
// minimise comb-flutter coincidences. At 44.1 kHz: 1543, 1697, 1873, ...
const DELAY_MS: [f32; N] = [35.0, 38.5, 42.5, 46.7, 51.3, 56.4, 62.0, 68.3];

struct Params {
    decay_pct: f32,
    damping: f32,
    pre_delay_ms: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 75.0,
            damping: 30.0,
            pre_delay_ms: 20.0,
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
            float_parameter("pre_delay_ms", "Pre-delay", None, Some(d.pre_delay_ms), 0.0, 100.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        decay_pct: required_f32(params, "decay").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        pre_delay_ms: required_f32(params, "pre_delay_ms").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// In-place 8-point Walsh-Hadamard transform with sqrt(8) normalisation.
/// The matrix is its own inverse (up to scale) and is unitary, so it
/// preserves energy across feedback iterations — the central reason
/// Jot picked Hadamard for FDN.
fn hadamard8(x: &mut [f32; N]) {
    // Stage 1: pairs (0,1), (2,3), (4,5), (6,7)
    for i in (0..N).step_by(2) {
        let a = x[i];
        let b = x[i + 1];
        x[i] = a + b;
        x[i + 1] = a - b;
    }
    // Stage 2: pairs (0,2), (1,3), (4,6), (5,7)
    for base in (0..N).step_by(4) {
        let a0 = x[base];
        let a1 = x[base + 1];
        let a2 = x[base + 2];
        let a3 = x[base + 3];
        x[base] = a0 + a2;
        x[base + 1] = a1 + a3;
        x[base + 2] = a0 - a2;
        x[base + 3] = a1 - a3;
    }
    // Stage 3: pairs (0,4), (1,5), (2,6), (3,7)
    for i in 0..4 {
        let a = x[i];
        let b = x[i + 4];
        x[i] = a + b;
        x[i + 4] = a - b;
    }
    // Normalise to unitary (1/sqrt(N)).
    let inv_sqrt_n = 1.0 / (N as f32).sqrt();
    for v in x.iter_mut() {
        *v *= inv_sqrt_n;
    }
}

struct DelayLine {
    buf: Vec<f32>,
    write_idx: usize,
}

impl DelayLine {
    fn new(samples: usize) -> Self {
        Self {
            buf: vec![0.0; samples.max(1)],
            write_idx: 0,
        }
    }
    fn read(&self) -> f32 {
        // Read from the oldest sample (write_idx is where we'll write next).
        self.buf[self.write_idx]
    }
    fn write(&mut self, v: f32) {
        self.buf[self.write_idx] = v;
        self.write_idx = (self.write_idx + 1) % self.buf.len();
    }
}

/// One-pole lowpass filter, used for damping per delay line.
struct OnePoleLpf {
    state: f32,
    coeff: f32,
}

impl OnePoleLpf {
    fn new() -> Self {
        Self { state: 0.0, coeff: 0.0 }
    }
    /// `damping` 0..1 → cutoff drops as damping grows. coeff = damping^0.5
    /// roughly maps to a useful range without exposing cutoff Hz.
    fn set_damping(&mut self, damping: f32) {
        self.coeff = damping.clamp(0.0, 1.0).sqrt();
    }
    fn process(&mut self, x: f32) -> f32 {
        // y[n] = (1-c) * x[n] + c * y[n-1]
        self.state = (1.0 - self.coeff).mul_add(x, self.coeff * self.state);
        self.state
    }
}

struct FdnReverb {
    params: Params,
    pre_delay: DelayLine,
    delays: [DelayLine; N],
    lpfs: [OnePoleLpf; N],
    feedback: f32,
}

impl FdnReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let pre_delay_samples = ((params.pre_delay_ms / 1000.0) * sample_rate) as usize;
        let pre_delay = DelayLine::new(pre_delay_samples.max(1));

        // Map decay_pct to feedback gain. 0% → quick decay (~0.5), 100% → very long (~0.97).
        let feedback = (0.5 + params.decay_pct * 0.47).clamp(0.0, 0.97);

        let delays: [DelayLine; N] = std::array::from_fn(|i| {
            let s = ((DELAY_MS[i] / 1000.0) * sample_rate) as usize;
            DelayLine::new(s)
        });
        let lpfs: [OnePoleLpf; N] = std::array::from_fn(|_| {
            let mut f = OnePoleLpf::new();
            f.set_damping(params.damping);
            f
        });

        Self {
            params,
            pre_delay,
            delays,
            lpfs,
            feedback,
        }
    }
}

impl StereoProcessor for FdnReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        // Pre-delay on the mono sum.
        let in_mono = (input[0] + input[1]) * 0.5;
        let pre = self.pre_delay.read();
        self.pre_delay.write(in_mono);

        // Read all delay outputs.
        let mut s: [f32; N] = std::array::from_fn(|i| self.delays[i].read());

        // Stereo wet output: alternate even/odd taps for L/R for natural width.
        let wet_l = (s[0] + s[2] + s[4] + s[6]) * 0.25;
        let wet_r = (s[1] + s[3] + s[5] + s[7]) * 0.25;

        // Damping inside the loop.
        for i in 0..N {
            s[i] = self.lpfs[i].process(s[i]) * self.feedback;
        }

        // Hadamard mixing matrix.
        hadamard8(&mut s);

        // Inject pre-delayed input across all lines and write back.
        for i in 0..N {
            let inject = pre * 0.25;
            self.delays[i].write(s[i] + inject);
        }

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct FdnAsMono(FdnReverb);

impl MonoProcessor for FdnAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(FdnReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(FdnAsMono(FdnReverb::new(p, sample_rate))))),
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

    fn default_reverb() -> FdnReverb {
        FdnReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn hadamard_is_self_inverse_up_to_scale() {
        let mut x: [f32; N] = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let original = x;
        hadamard8(&mut x);
        hadamard8(&mut x);
        // H * H = I (with our 1/sqrt(N) normalisation per call → /N total).
        // After two calls the result equals original input.
        for i in 0..N {
            assert!(
                (x[i] - original[i]).abs() < 1e-4,
                "h(h(x))[{i}] = {} != {}",
                x[i], original[i],
            );
        }
    }

    #[test]
    fn hadamard_preserves_energy() {
        let mut x: [f32; N] = [0.5, -0.3, 1.0, 0.0, 0.7, -0.2, 0.4, -0.1];
        let energy_in: f32 = x.iter().map(|v| v * v).sum();
        hadamard8(&mut x);
        let energy_out: f32 = x.iter().map(|v| v * v).sum();
        assert!(
            (energy_in - energy_out).abs() < 1e-5,
            "energy in {} != energy out {}",
            energy_in, energy_out
        );
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
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin();
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite() && r.is_finite());
            if l.abs() > 1e-6 || r.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "expected non-zero output");
    }

    #[test]
    fn mono_adapter_runs_silence_and_sine() {
        let mut mono = FdnAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sr).sin();
            let out = MonoProcessor::process_sample(&mut mono, s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 {
                any_nonzero = true;
            }
        }
        assert!(any_nonzero, "mono adapter expected non-zero output");
    }
}
