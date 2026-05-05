//! Spring reverb — physical model after:
//! - Parker, J. (2010). "Spring Reverberation: A Physical Perspective".
//!   Proc. of the 13th Int. Conference on Digital Audio Effects (DAFx-10).
//! - Välimäki, V., Parker, J., Abel, J. S. (2010). "Parametric Spring
//!   Reverberation Effect". JAES Vol. 58 No. 7/8.
//!
//! The spring's longitudinal wave equation produces *dispersive*
//! propagation — high frequencies travel faster than low — which gives
//! the characteristic "boing" chirp on every bounce. We model the
//! dispersion with a long cascade of first-order allpass filters
//! (Smith's classic technique for cheap allpass dispersion), then a
//! feedback delay loop for the multiple bounces, and an input/output
//! bandpass to limit the spring's effective audio range.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "spring_parker_2010";
pub const DISPLAY_NAME: &str = "Spring (Parker 2010)";

const N_ALLPASS: usize = 80;
const ALLPASS_COEF: f32 = 0.6;

struct Params {
    decay_pct: f32,
    damping: f32,
    boing: f32,         // 0..1 — feedback amount of the "spring" loop
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 60.0,
            damping: 30.0,
            boing: 70.0,
            mix: 35.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let d = Params::default();
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("decay", "Decay", None, Some(d.decay_pct), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(d.damping), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("boing", "Boing", None, Some(d.boing), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        decay_pct: required_f32(params, "decay").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        boing: required_f32(params, "boing").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// First-order allpass: y[n] = -g·x[n] + x[n-1] + g·y[n-1].
/// Group delay is frequency-dependent → cascading these produces a
/// dispersive (chirpy) response, which is exactly the spring's signature.
struct AllpassDispersion {
    g: f32,
    x_prev: f32,
    y_prev: f32,
}
impl AllpassDispersion {
    fn new(g: f32) -> Self { Self { g, x_prev: 0.0, y_prev: 0.0 } }
    fn process(&mut self, x: f32) -> f32 {
        let y = -self.g * x + self.x_prev + self.g * self.y_prev;
        self.x_prev = x;
        self.y_prev = y;
        y
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

/// Two-pole bandpass: simple SVF-ish bandpass, peak at f_c with
/// resonance Q. Implementation uses Chamberlin biquad.
struct Bandpass {
    f: f32,         // pre-warped frequency
    q: f32,         // 1/Q
    low: f32,
    band: f32,
}
impl Bandpass {
    fn new(cutoff_hz: f32, q: f32, sample_rate: f32) -> Self {
        let f = 2.0 * (std::f32::consts::PI * cutoff_hz / sample_rate).sin();
        Self { f, q: 1.0 / q.max(0.1), low: 0.0, band: 0.0 }
    }
    fn process(&mut self, x: f32) -> f32 {
        // Chamberlin SVF (one iteration; for stability at moderate freqs).
        self.low += self.f * self.band;
        let high = x - self.low - self.q * self.band;
        self.band += self.f * high;
        // Bandpass output = self.band
        self.band
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

struct SpringReverb {
    params: Params,
    in_bandpass: Bandpass,
    allpasses: Vec<AllpassDispersion>,
    delay: DelayLine,
    damping_lp: OnePoleLpf,
    feedback: f32,
    out_bandpass_l: Bandpass,
    out_bandpass_r: Bandpass,
}

impl SpringReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        // Spring effective length ~ 30ms one-way. With dispersion the
        // bounces overlap into a continuous chirpy tail.
        let delay_samples = (sample_rate * 0.030) as usize;
        let feedback = (0.4 + params.boing * 0.55).clamp(0.0, 0.95);

        let mut damping_lp = OnePoleLpf::new();
        damping_lp.set_damping(params.damping);

        let allpasses = (0..N_ALLPASS)
            .map(|_| AllpassDispersion::new(ALLPASS_COEF))
            .collect();

        Self {
            params,
            in_bandpass: Bandpass::new(800.0, 0.7, sample_rate),
            allpasses,
            delay: DelayLine::new(delay_samples),
            damping_lp,
            feedback,
            out_bandpass_l: Bandpass::new(750.0, 0.8, sample_rate),
            out_bandpass_r: Bandpass::new(850.0, 0.8, sample_rate),
        }
    }
}

impl StereoProcessor for SpringReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono_in = (input[0] + input[1]) * 0.5;

        // Input bandpass — spring rejects content outside ~200-5000 Hz.
        let bp_in = self.in_bandpass.process(mono_in) * 4.0; // bandpass loses energy

        // Sum input with feedback through dispersion + damping.
        let from_loop = self.delay.read();
        let damped = self.damping_lp.process(from_loop);
        let into_chain = bp_in + damped * self.feedback * (0.5 + self.params.decay_pct * 0.45);

        // Allpass cascade — the dispersion is what makes the spring "boing".
        let mut x = into_chain;
        for ap in &mut self.allpasses {
            x = ap.process(x);
        }
        self.delay.write(x);

        // Output bandpass per side — slightly different cutoffs for natural
        // L/R variation since a real spring tank has two pickups at slightly
        // different positions.
        let wet_l = self.out_bandpass_l.process(x) * 4.0;
        let wet_r = self.out_bandpass_r.process(x) * 4.0;

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct SpringAsMono(SpringReverb);

impl MonoProcessor for SpringAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(SpringReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(SpringAsMono(SpringReverb::new(p, sample_rate))))),
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
#[path = "native_spring_parker_2010_tests.rs"]
mod tests;
