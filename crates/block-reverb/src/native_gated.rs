//! Gated reverb — dense Hall-style tail multiplied by a hard envelope
//! (hold + fast release) so the reverb cuts off abruptly while the dry
//! transient is still ringing. The signature 1980s "Phil Collins snare"
//! sound, originally a happy accident on the SSL G-series talkback
//! reverb at AIR Studios (Hugh Padgham / Steve Lillywhite, 1980).
//!
//! References:
//! - Schroeder 1962 / Moorer 1979 — comb+allpass underlying topology
//! - Padgham/Lillywhite "Phil Collins drum sound" technique
//!
//! Implementation: comb+allpass network (Freeverb-style 8 combs / 4
//! allpasses per side) for the reverb itself; an envelope follower on
//! the input dry signal triggers a gate state machine (open → hold →
//! release) which multiplies the wet output. RT-safe.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "gated";
pub const DISPLAY_NAME: &str = "Gated Reverb";

const COMB_SIZES: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_SIZES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const FIXED_GAIN: f32 = 0.015;

struct Params {
    decay_pct: f32,    // 0..1 → comb feedback
    damping: f32,      // 0..1
    hold_ms: f32,
    release_ms: f32,
    threshold_lin: f32, // input envelope level above which gate opens
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 0.85,
            damping: 0.30,
            hold_ms: 200.0,
            release_ms: 30.0,
            threshold_lin: 0.05,
            mix: 50.0 / 100.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("decay", "Decay", None, Some(85.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(30.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("hold_ms", "Hold", None, Some(200.0), 30.0, 800.0, 5.0, ParameterUnit::Milliseconds),
            float_parameter("release_ms", "Release", None, Some(30.0), 5.0, 200.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("threshold", "Threshold", None, Some(5.0), 0.5, 50.0, 0.5, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        decay_pct: required_f32(params, "decay").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        hold_ms: required_f32(params, "hold_ms").map_err(Error::msg)?,
        release_ms: required_f32(params, "release_ms").map_err(Error::msg)?,
        threshold_lin: required_f32(params, "threshold").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

#[derive(PartialEq, Eq)]
enum GateState {
    Closed,
    Hold,
    Release,
}

struct Gate {
    state: GateState,
    samples_left: u32,
    hold_samples: u32,
    release_samples: u32,
    threshold: f32,
    env: f32,           // smoothed input envelope follower
    env_attack: f32,
    env_release: f32,
    gain: f32,          // current gate gain 0..1
}

impl Gate {
    fn new(sample_rate: f32, hold_ms: f32, release_ms: f32, threshold: f32) -> Self {
        let hold_samples = ((hold_ms / 1000.0) * sample_rate) as u32;
        let release_samples = ((release_ms / 1000.0) * sample_rate).max(1.0) as u32;
        Self {
            state: GateState::Closed,
            samples_left: 0,
            hold_samples,
            release_samples,
            threshold: threshold.max(1e-6),
            env: 0.0,
            env_attack: 0.99,  // fast attack on follower
            env_release: 0.999, // slower release
            gain: 0.0,
        }
    }

    fn step(&mut self, input_sample: f32) -> f32 {
        // Envelope follower (peak-style, asymmetric).
        let abs = input_sample.abs();
        if abs > self.env {
            self.env = abs.mul_add(1.0 - self.env_attack, self.env * self.env_attack);
        } else {
            self.env = abs.mul_add(1.0 - self.env_release, self.env * self.env_release);
        }

        match self.state {
            GateState::Closed => {
                if self.env > self.threshold {
                    self.state = GateState::Hold;
                    self.samples_left = self.hold_samples;
                    self.gain = 1.0;
                }
            }
            GateState::Hold => {
                self.gain = 1.0;
                if self.samples_left > 0 {
                    self.samples_left -= 1;
                } else {
                    self.state = GateState::Release;
                    self.samples_left = self.release_samples;
                }
                // Re-trigger if input is loud again.
                if self.env > self.threshold {
                    self.samples_left = self.hold_samples;
                }
            }
            GateState::Release => {
                if self.samples_left > 0 {
                    self.gain = self.samples_left as f32 / self.release_samples as f32;
                    self.samples_left -= 1;
                } else {
                    self.gain = 0.0;
                    self.state = GateState::Closed;
                }
                // Re-trigger from release back to hold if loud input returns.
                if self.env > self.threshold {
                    self.state = GateState::Hold;
                    self.samples_left = self.hold_samples;
                    self.gain = 1.0;
                }
            }
        }
        self.gain
    }
}

struct GatedReverb {
    params: Params,
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
    gate: Gate,
}

impl GatedReverb {
    fn new(params: Params, sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        // Map decay 0..1 to feedback 0.7..0.95 (less than Freeverb to keep
        // the underlying tail short — gating shapes the perceived length).
        let feedback = 0.7 + params.decay_pct * 0.25;
        let damping = params.damping * 0.4;

        let mut combs_l: Vec<CombFilter> = COMB_SIZES
            .iter()
            .map(|&s| CombFilter::new((s as f32 * scale) as usize))
            .collect();
        let mut combs_r: Vec<CombFilter> = COMB_SIZES
            .iter()
            .map(|&s| CombFilter::new(((s + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();
        for c in combs_l.iter_mut().chain(combs_r.iter_mut()) {
            c.set_feedback(feedback);
            c.set_damping(damping);
        }

        let allpasses_l: Vec<AllpassFilter> = ALLPASS_SIZES
            .iter()
            .map(|&s| AllpassFilter::new((s as f32 * scale) as usize))
            .collect();
        let allpasses_r: Vec<AllpassFilter> = ALLPASS_SIZES
            .iter()
            .map(|&s| AllpassFilter::new(((s + STEREO_SPREAD) as f32 * scale) as usize))
            .collect();

        let gate = Gate::new(sample_rate, params.hold_ms, params.release_ms, params.threshold_lin);

        Self {
            params,
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
            gate,
        }
    }
}

impl StereoProcessor for GatedReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono_in = (input[0] + input[1]) * FIXED_GAIN;

        let wet_l_sum: f32 = self.combs_l.iter_mut().map(|c| c.process(mono_in)).sum();
        let wet_r_sum: f32 = self.combs_r.iter_mut().map(|c| c.process(mono_in)).sum();

        let mut wet_l = wet_l_sum;
        for ap in &mut self.allpasses_l {
            wet_l = ap.process(wet_l);
        }
        let mut wet_r = wet_r_sum;
        for ap in &mut self.allpasses_r {
            wet_r = ap.process(wet_r);
        }

        let gate_gain = self.gate.step((input[0] + input[1]) * 0.5);
        wet_l *= gate_gain;
        wet_r *= gate_gain;

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l),
            dry.mul_add(input[1], self.params.mix * wet_r),
        ]
    }
}

struct GatedAsMono(GatedReverb);

impl MonoProcessor for GatedAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(GatedReverb::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(GatedAsMono(GatedReverb::new(p, sample_rate))))),
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

// ── filters (same Schroeder-Moorer building blocks used elsewhere) ──

struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    filter_store: f32,
    damping: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback: 0.84,
            filter_store: 0.0,
            damping: 0.2,
        }
    }
    fn set_feedback(&mut self, fb: f32) { self.feedback = fb; }
    fn set_damping(&mut self, d: f32) { self.damping = d.clamp(0.0, 1.0); }
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = output * (1.0 - self.damping) + self.filter_store * self.damping;
        self.buffer[self.index] = input + self.filter_store * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self { buffer: vec![0.0; size.max(1)], index: 0 }
    }
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + buffered * 0.5;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_reverb() -> GatedReverb {
        GatedReverb::new(Params::default(), 44_100.0)
    }

    #[test]
    fn silence_input_keeps_gate_closed() {
        let mut reverb = default_reverb();
        for i in 0..2048 {
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() < 1e-9 && r.abs() < 1e-9, "gate must stay closed under silence (sample {i})");
        }
    }

    #[test]
    fn loud_burst_opens_gate_then_releases() {
        let mut reverb = default_reverb();
        let sr = 44_100.0_f32;
        // 50ms loud sine to trip the gate, then silence for >hold+release.
        let trigger_samples = (sr * 0.05) as usize;
        for i in 0..trigger_samples {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.8;
            let [l, _r] = StereoProcessor::process_frame(&mut reverb, [s, s]);
            assert!(l.is_finite());
        }
        // After ~hold (200ms) + release (30ms) + margin, gate gain should be back near 0.
        let settle = (sr * 0.6) as usize;
        let mut last_wet = 1.0_f32;
        for _ in 0..settle {
            let [l, _r] = StereoProcessor::process_frame(&mut reverb, [0.0, 0.0]);
            last_wet = l;
        }
        assert!(last_wet.abs() < 1e-3, "gate should have closed by now (got {last_wet})");
    }

    #[test]
    fn impulse_response_finite() {
        let mut reverb = default_reverb();
        for i in 0..44_100 {
            let input = if i < 100 { 0.5 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at sample {i}");
        }
    }

    #[test]
    fn mono_adapter_finite_under_burst() {
        let mut mono = GatedAsMono(default_reverb());
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.6;
            assert!(MonoProcessor::process_sample(&mut mono, s).is_finite());
        }
    }
}
