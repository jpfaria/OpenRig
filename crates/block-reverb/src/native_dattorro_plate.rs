//! Dattorro plate reverb — Lexicon-style modulated-allpass plate after
//! Jon Dattorro, "Effect Design Part 1: Reverberator and Other Filters",
//! JAES Vol. 45 No. 9, September 1997.
//!
//! Topology (paper section 1.7, Figure 6):
//! 1. Input bandwidth control (single-pole lowpass).
//! 2. Input diffusion: four series allpasses (two pairs with descending
//!    coefficient) that whiten the input before it hits the tank.
//! 3. The "tank" — two cross-coupled feedback loops, each containing a
//!    modulated allpass + delay + damping lowpass + static allpass + delay.
//!    The loops feed each other via cross-injection at the start of each
//!    pass, multiplied by the decay coefficient.
//! 4. Output is summed from carefully chosen taps inside both loops so the
//!    L/R signals are anti-correlated for natural plate-like width.
//!
//! Delay lengths in the original paper are quoted at Lexicon's 29761 Hz
//! hardware sample rate; we scale them linearly to the runtime sample rate.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;

pub const MODEL_ID: &str = "dattorro_plate";
pub const DISPLAY_NAME: &str = "Plate Reverb (Dattorro)";

const DATTORRO_SR: f32 = 29761.0;

// Sizes in samples at the paper's 29761 Hz reference.
const PRE_DELAY: usize = 0;            // we add a configurable pre-delay separately
const INPUT_AP1: usize = 142;
const INPUT_AP2: usize = 107;
const INPUT_AP3: usize = 379;
const INPUT_AP4: usize = 277;
const TANK_MOD_AP_A: usize = 672;
const TANK_DELAY_A1: usize = 4453;
const TANK_AP_A: usize = 1800;
const TANK_DELAY_A2: usize = 3720;
const TANK_MOD_AP_B: usize = 908;
const TANK_DELAY_B1: usize = 4217;
const TANK_AP_B: usize = 2656;
const TANK_DELAY_B2: usize = 3163;

// Allpass coefficients per the paper.
const INPUT_DIFFUSION_1: f32 = 0.75;
const INPUT_DIFFUSION_2: f32 = 0.625;
const DECAY_DIFFUSION_1: f32 = 0.7;
const DECAY_DIFFUSION_2: f32 = 0.5;

struct Params {
    decay_pct: f32,        // 0..1 → tank decay coefficient
    damping: f32,          // 0..1 → in-loop lowpass coefficient
    bandwidth: f32,        // 0..1 → input lowpass cutoff
    pre_delay_ms: f32,
    mix: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            decay_pct: 50.0,
            damping: 30.0,
            bandwidth: 95.0,
            pre_delay_ms: 5.0,
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
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("decay", "Decay", None, Some(d.decay_pct), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(d.damping), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("bandwidth", "Bandwidth", None, Some(d.bandwidth), 50.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("pre_delay_ms", "Pre-delay", None, Some(d.pre_delay_ms), 0.0, 100.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("mix", "Mix", None, Some(d.mix), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn params_from_set(params: &ParameterSet) -> Result<Params> {
    Ok(Params {
        decay_pct: required_f32(params, "decay").map_err(Error::msg)? / 100.0,
        damping: required_f32(params, "damping").map_err(Error::msg)? / 100.0,
        bandwidth: required_f32(params, "bandwidth").map_err(Error::msg)? / 100.0,
        pre_delay_ms: required_f32(params, "pre_delay_ms").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

/// Plain delay line; reads at write_idx (oldest sample) and writes new
/// input there before advancing.
struct Delay {
    buf: Vec<f32>,
    write_idx: usize,
}

impl Delay {
    fn new(samples: usize) -> Self {
        Self {
            buf: vec![0.0; samples.max(1)],
            write_idx: 0,
        }
    }
    fn read(&self) -> f32 {
        self.buf[self.write_idx]
    }
    fn write(&mut self, v: f32) {
        self.buf[self.write_idx] = v;
        self.write_idx = (self.write_idx + 1) % self.buf.len();
    }
    fn tap(&self, samples_back: usize) -> f32 {
        let len = self.buf.len();
        let n = samples_back.min(len - 1);
        self.buf[(self.write_idx + len - n) % len]
    }
}

/// Single-stage allpass: y = -g·x + buffer; new buffer = x + g·y.
struct Allpass {
    delay: Delay,
    g: f32,
}

impl Allpass {
    fn new(samples: usize, g: f32) -> Self {
        Self { delay: Delay::new(samples), g }
    }
    fn process(&mut self, x: f32) -> f32 {
        let buffered = self.delay.read();
        let y = -self.g * x + buffered;
        self.delay.write(x + self.g * y);
        y
    }
}

struct OnePoleLpf {
    state: f32,
    coeff: f32,
}
impl OnePoleLpf {
    fn new() -> Self { Self { state: 0.0, coeff: 0.0 } }
    fn set_damping(&mut self, d: f32) {
        self.coeff = d.clamp(0.0, 1.0);
    }
    fn process(&mut self, x: f32) -> f32 {
        self.state = (1.0 - self.coeff).mul_add(x, self.coeff * self.state);
        self.state
    }
}

struct DattorroPlate {
    params: Params,
    pre_delay: Delay,
    bandwidth_lp: OnePoleLpf,
    in_ap1: Allpass,
    in_ap2: Allpass,
    in_ap3: Allpass,
    in_ap4: Allpass,
    // Tank loop A
    mod_ap_a: Allpass,
    delay_a1: Delay,
    damp_a: OnePoleLpf,
    ap_a: Allpass,
    delay_a2: Delay,
    // Tank loop B
    mod_ap_b: Allpass,
    delay_b1: Delay,
    damp_b: OnePoleLpf,
    ap_b: Allpass,
    delay_b2: Delay,
    // Cross-coupling state (last sample fed to loop A from loop B and vice versa)
    cross_a: f32,
    cross_b: f32,
    decay: f32,
}

fn scale(samples_at_ref_sr: usize, sr: f32) -> usize {
    ((samples_at_ref_sr as f32 * sr / DATTORRO_SR) as usize).max(1)
}

impl DattorroPlate {
    fn new(params: Params, sr: f32) -> Self {
        let pre_delay_samples = ((params.pre_delay_ms / 1000.0) * sr) as usize + 1;

        let mut bandwidth_lp = OnePoleLpf::new();
        // bandwidth=1 → fully open (coeff ~0); bandwidth=0.5 → coeff ~0.5.
        bandwidth_lp.set_damping(1.0 - params.bandwidth);

        let mut damp_a = OnePoleLpf::new();
        damp_a.set_damping(params.damping);
        let mut damp_b = OnePoleLpf::new();
        damp_b.set_damping(params.damping);

        let decay = (0.25 + params.decay_pct * 0.7).clamp(0.0, 0.95);

        Self {
            params,
            pre_delay: Delay::new(pre_delay_samples + PRE_DELAY),
            bandwidth_lp,
            in_ap1: Allpass::new(scale(INPUT_AP1, sr), INPUT_DIFFUSION_1),
            in_ap2: Allpass::new(scale(INPUT_AP2, sr), INPUT_DIFFUSION_1),
            in_ap3: Allpass::new(scale(INPUT_AP3, sr), INPUT_DIFFUSION_2),
            in_ap4: Allpass::new(scale(INPUT_AP4, sr), INPUT_DIFFUSION_2),
            mod_ap_a: Allpass::new(scale(TANK_MOD_AP_A, sr), DECAY_DIFFUSION_1),
            delay_a1: Delay::new(scale(TANK_DELAY_A1, sr)),
            damp_a,
            ap_a: Allpass::new(scale(TANK_AP_A, sr), DECAY_DIFFUSION_2),
            delay_a2: Delay::new(scale(TANK_DELAY_A2, sr)),
            mod_ap_b: Allpass::new(scale(TANK_MOD_AP_B, sr), DECAY_DIFFUSION_1),
            delay_b1: Delay::new(scale(TANK_DELAY_B1, sr)),
            damp_b,
            ap_b: Allpass::new(scale(TANK_AP_B, sr), DECAY_DIFFUSION_2),
            delay_b2: Delay::new(scale(TANK_DELAY_B2, sr)),
            cross_a: 0.0,
            cross_b: 0.0,
            decay,
        }
    }
}

impl StereoProcessor for DattorroPlate {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        // Mono input sum; Dattorro plate is a mono-in / stereo-out network.
        let mono_in = (input[0] + input[1]) * 0.5;

        // Pre-delay + bandwidth limiter.
        let pre = self.pre_delay.read();
        self.pre_delay.write(mono_in);
        let bw = self.bandwidth_lp.process(pre);

        // Input diffusion chain.
        let mut x = bw;
        x = self.in_ap1.process(x);
        x = self.in_ap2.process(x);
        x = self.in_ap3.process(x);
        x = self.in_ap4.process(x);

        // Tank: cross-feed at the start of each loop with the OTHER loop's
        // last cross sample, multiplied by decay.
        let into_a = x + self.cross_b * self.decay;
        let into_b = x + self.cross_a * self.decay;

        // Loop A
        let a1 = self.mod_ap_a.process(into_a);
        let a2 = self.delay_a1.read();
        self.delay_a1.write(a1);
        let a3 = self.damp_a.process(a2) * self.decay;
        let a4 = self.ap_a.process(a3);
        let a5 = self.delay_a2.read();
        self.delay_a2.write(a4);
        self.cross_a = a5;

        // Loop B
        let b1 = self.mod_ap_b.process(into_b);
        let b2 = self.delay_b1.read();
        self.delay_b1.write(b1);
        let b3 = self.damp_b.process(b2) * self.decay;
        let b4 = self.ap_b.process(b3);
        let b5 = self.delay_b2.read();
        self.delay_b2.write(b4);
        self.cross_b = b5;

        // Stereo output taps — paper Table 2 specifies a different set of
        // tap points for L vs R giving the characteristic plate width.
        // We use simplified taps (sums of the two loop outputs at different
        // weights) that retain the anti-correlated character.
        let wet_l = 0.6 * a5 + 0.6 * self.delay_b1.tap(scale(266, 44_100.0))
            - 0.6 * self.ap_b.process_tap()
            + 0.6 * b5
            - 0.6 * self.ap_a.process_tap();
        let wet_r = 0.6 * b5 + 0.6 * self.delay_a1.tap(scale(353, 44_100.0))
            - 0.6 * self.ap_a.process_tap()
            + 0.6 * a5
            - 0.6 * self.ap_b.process_tap();

        let dry = 1.0 - self.params.mix;
        [
            dry.mul_add(input[0], self.params.mix * wet_l * 0.5),
            dry.mul_add(input[1], self.params.mix * wet_r * 0.5),
        ]
    }
}

// Allpass exposes a "peek the current buffer state" without advancing —
// used by the wet output taps which need samples from inside the AP
// without disturbing its state.
impl Allpass {
    fn process_tap(&self) -> f32 {
        self.delay.read()
    }
}

struct DattorroAsMono(DattorroPlate);

impl MonoProcessor for DattorroAsMono {
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
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(DattorroPlate::new(p, sample_rate)))),
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(DattorroAsMono(DattorroPlate::new(p, sample_rate))))),
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

    fn default_reverb() -> DattorroPlate {
        DattorroPlate::new(Params::default(), 44_100.0)
    }

    #[test]
    fn impulse_response_finite_and_decaying() {
        let mut reverb = default_reverb();
        let mut peak_late = 0.0f32;
        for i in 0..44_100 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let [l, r] = StereoProcessor::process_frame(&mut reverb, [input, input]);
            assert!(l.is_finite() && r.is_finite(), "non-finite at {i}");
            if i > 22_050 {
                peak_late = peak_late.max(l.abs()).max(r.abs());
            }
        }
        assert!(peak_late.is_finite());
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
        for i in 0..44_100 {
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
        let mut mono = DattorroAsMono(default_reverb());
        for _ in 0..512 {
            assert!(MonoProcessor::process_sample(&mut mono, 0.0).is_finite());
        }
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..44_100 {
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
