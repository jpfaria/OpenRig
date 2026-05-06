//! Cathedral reverb — very long decay (6-10s), high modal density, bright
//! tail with controllable HF damping. Implemented as a 16-channel FDN
//! after Jot 1991 with long delay lengths and a long pre-delay so the
//! "first arrival" feels distant from the dry signal.
//!
//! References:
//! - Jot, Jean-Marc. "An Analysis/Synthesis Approach to Real-Time Artificial
//!   Reverberation" (ICASSP 1992).
//! - Smith, Julius O. "Physical Audio Signal Processing" — chapter on
//!   FDN and physical-room modelling.
//!
//! 16 lines (vs 8 in `native_fdn_jot`) doubles the modal density which is
//! perceptually important for very long tails — short tails sound similar
//! at 8 vs 16, but as the decay extends past ~3s, an 8-line FDN starts
//! exposing its modes audibly. 16 lines pushes that ceiling well past 10s.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "cathedral";
pub const DISPLAY_NAME: &str = "Cathedral";

const N: usize = 16;

// Long delay lengths in ms — co-prime-ish, spread between 60ms and 180ms
// so the first reflection density is high without identical-period collisions.
const DELAY_MS: [f32; N] = [
    60.3, 67.7, 75.1, 82.9, 91.3, 100.7, 110.3, 120.7,
    131.9, 143.3, 155.7, 168.3, 178.9, 162.1, 148.7, 135.3,
];

struct Params {
    decay_pct: f32,    // 0..1 → maps to feedback giving 3..10s decay
    damping: f32,
    pre_delay_ms: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 80.0,
            damping: 25.0,
            pre_delay_ms: 80.0,
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
            float_parameter("pre_delay_ms", "Pre-delay", None, Some(d.pre_delay_ms), 0.0, 200.0, 1.0, ParameterUnit::Milliseconds),
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

/// In-place 16-point Walsh-Hadamard transform with 1/sqrt(N) normalisation.
fn hadamard16(x: &mut [f32; N]) {
    // 4 butterfly stages, doubling stride each step.
    let mut stride = 1;
    while stride < N {
        let block = stride * 2;
        let mut base = 0;
        while base < N {
            for i in 0..stride {
                let a = x[base + i];
                let b = x[base + i + stride];
                x[base + i] = a + b;
                x[base + i + stride] = a - b;
            }
            base += block;
        }
        stride *= 2;
    }
    let inv = 1.0 / (N as f32).sqrt();
    for v in x.iter_mut() {
        *v *= inv;
    }
}

struct CathedralReverb {
    params: Params,
    pre_delay: DelayLine,
    delays: [DelayLine; N],
    lpfs: [OnePoleLpf; N],
    feedback: f32,
}

impl CathedralReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let pre_delay_samples =
            ((params.pre_delay_ms.max(0.0) / 1000.0) * sample_rate) as usize + 1;
        // Map decay_pct to feedback. Cathedral wants long tails — 0% → 0.85,
        // 100% → 0.985 (close to but not at 1.0 so it stays stable).
        let feedback = (0.85 + params.decay_pct * 0.135).clamp(0.0, 0.985);

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
            pre_delay: DelayLine::new(pre_delay_samples),
            delays,
            lpfs,
            feedback,
        }
    }
}

impl StereoProcessor for CathedralReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let in_mono = (input[0] + input[1]) * 0.5;
        let pre = self.pre_delay.read();
        self.pre_delay.write(in_mono);

        let mut s: [f32; N] = std::array::from_fn(|i| self.delays[i].read());

        // Stereo wet: even taps → L, odd taps → R for natural width.
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        for i in 0..N {
            if i % 2 == 0 { wet_l += s[i]; } else { wet_r += s[i]; }
        }
        wet_l *= 2.0 / N as f32;
        wet_r *= 2.0 / N as f32;

        for i in 0..N {
            s[i] = self.lpfs[i].process(s[i]) * self.feedback;
        }
        hadamard16(&mut s);

        for i in 0..N {
            self.delays[i].write(s[i] + pre * (1.0 / N as f32));
        }

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct CathedralAsMono(CathedralReverb);

impl MonoProcessor for CathedralAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(CathedralReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(CathedralAsMono(CathedralReverb::new(p, sample_rate))))),
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
#[path = "native_cathedral_tests.rs"]
mod tests;
