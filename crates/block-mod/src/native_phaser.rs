//! Phaser — 6-stage cascade of first-order all-pass filters with
//! LFO-modulated cutoff, summed (with feedback) into the dry signal.
//! Pro-tier.
//!
//! Reference: Smith, J. O. III. "Physical Audio Signal Processing,"
//! chapter on Time-Varying Allpass Filters (CCRMA online). MXR
//! Phase 90 schematic analysis at electrosmash.com.
//!
//! Pro-tier topology:
//!   1. LFO with PolyBLEP (Lfo, Sine shape) — alias-free modulator.
//!   2. JFET-like non-linear sweep curve. Real Phase 90 uses a 2N5952
//!      JFET as a voltage-controlled resistor; pinch-off gives an
//!      exponential resistance/voltage relationship that warps the
//!      sweep towards the upper end. We approximate with a tanh-like
//!      skew applied on top of the log-linear sweep.
//!   3. tanh-saturated feedback. Real op-amp feedback in the original
//!      pedal soft-clips at high resonance — emulating it tames runaway
//!      and adds the "sing" the analog circuit is loved for.
//!   4. DC blocker on the feedback register so any tanh asymmetry
//!      cannot drift the chain.
//!   5. Anti-denormal flush on the feedback state.
//!
//! RT-safe: stack-allocated stage state, single tanh per sample on
//! the feedback path. Feedback is bounded by tanh saturation so BIBO
//! is unconditional.

use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::{Error, Result};
use block_core::dsp::{flush_denormal, DcBlocker, Lfo, LfoShape};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::PI;

pub const MODEL_ID: &str = "phaser_classic";
pub const DISPLAY_NAME: &str = "Classic Phaser";

const STAGES: usize = 6;
const MIN_FREQ_HZ: f32 = 200.0;
const MAX_FREQ_HZ: f32 = 2_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaserParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl Default for PhaserParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.5,
            depth: 70.0,
            feedback: 50.0,
            mix: 50.0,
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
                "rate_hz",
                "Rate",
                None,
                Some(PhaserParams::default().rate_hz),
                0.05,
                8.0,
                0.05,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(PhaserParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(PhaserParams::default().feedback),
                -95.0,
                95.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(PhaserParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<PhaserParams> {
    Ok(PhaserParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)? / 100.0,
        feedback: required_f32(params, "feedback").map_err(Error::msg)? / 100.0,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

pub struct Phaser {
    depth: f32,
    feedback: f32,
    mix: f32,
    sample_rate: f32,
    lfo: Lfo,
    stage_z: [f32; STAGES],
    feedback_z: f32,
    feedback_dc_blocker: DcBlocker,
}

impl Phaser {
    pub fn new(rate_hz: f32, depth: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        Self {
            depth: depth.clamp(0.0, 1.0),
            feedback: feedback.clamp(-0.95, 0.95),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            lfo: Lfo::new(LfoShape::Sine, rate_hz, sample_rate),
            stage_z: [0.0; STAGES],
            feedback_z: 0.0,
            feedback_dc_blocker: DcBlocker::new(5.0, sample_rate),
        }
    }

    pub fn set_lfo_phase(&mut self, phase: f32) {
        self.lfo.set_phase(phase);
    }

    /// JFET-like sweep skew. `t` in [0, 1] (raw LFO unipolar). Output
    /// in [0, 1] but biased toward the upper portion of the sweep,
    /// emulating the exponential V→R curve of the 2N5952 in pinch-off.
    fn shape_sweep(t: f32) -> f32 {
        // tanh skew normalised so the curve passes exactly through
        // (0,0), (0.5,0.5) and (1,1) — only the slope is shaped, not
        // the endpoints.
        const K: f32 = 2.5;
        let centered = t * 2.0 - 1.0;
        let denom = K.tanh();
        let raw = (centered * K).tanh() / denom * 0.5 + 0.5;
        // Blend 70% shaped + 30% linear so the bottom of the sweep
        // still feels musical (shaped-only collapses too fast).
        0.7 * raw + 0.3 * t
    }
}

impl MonoProcessor for Phaser {
    fn process_sample(&mut self, input: f32) -> f32 {
        let lfo_raw = self.lfo.next_unipolar();
        let lfo_shaped = Self::shape_sweep(lfo_raw);

        // Sweep break frequency on a log scale, then JFET-shape it.
        let log_min = MIN_FREQ_HZ.ln();
        let log_max = MAX_FREQ_HZ.ln();
        let log_freq = log_min + (log_max - log_min) * self.depth * lfo_shaped;
        let break_hz = log_freq.exp();

        // First-order all-pass coefficient via bilinear substitution:
        //   a = (tan(pi*f/fs) - 1) / (tan(pi*f/fs) + 1)
        let t = (PI * break_hz / self.sample_rate).tan();
        let a = (t - 1.0) / (t + 1.0);

        let mut x = input + self.feedback_z;
        for stage in self.stage_z.iter_mut() {
            // Direct Form II Transposed first-order all-pass.
            let y = a * x + *stage;
            *stage = x - a * y;
            x = y;
        }

        // Soft-sat the feedback — limits resonance, kills runaway, gives
        // the analog "sing" near max feedback.
        let fb_soft = (self.feedback * x * 1.5).tanh() / 1.5;
        // DC-block + denormal flush so feedback stays well-behaved.
        self.feedback_z = flush_denormal(self.feedback_dc_blocker.process(fb_soft));

        (1.0 - self.mix) * input + self.mix * x
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(Phaser::new(
        p.rate_hz,
        p.depth,
        p.feedback,
        p.mix,
        sample_rate,
    )))
}

pub fn build_processor_with_phase(
    params: &ParameterSet,
    sample_rate: f32,
    phase_offset: f32,
) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    let mut ph = Phaser::new(p.rate_hz, p.depth, p.feedback, p.mix, sample_rate);
    ph.set_lfo_phase(phase_offset / std::f32::consts::TAU);
    Ok(Box::new(ph))
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
            struct StereoPhaser {
                left: Box<dyn block_core::MonoProcessor>,
                right: Box<dyn block_core::MonoProcessor>,
            }

            impl block_core::StereoProcessor for StereoPhaser {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }

            Ok(block_core::BlockProcessor::Stereo(Box::new(StereoPhaser {
                left: build_processor(params, sample_rate)?,
                right: build_processor_with_phase(params, sample_rate, std::f32::consts::PI)?,
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
    use std::f32::consts::TAU;

    #[test]
    fn silence_in_silence_out() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        for _ in 0..4096 {
            let out = ph.process_sample(0.0);
            assert!(out.abs() < 1e-20, "phaser of silence: {out}");
        }
    }

    #[test]
    fn sine_input_output_finite() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.5, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..4096 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = ph.process_sample(input);
            assert!(out.is_finite(), "non-finite at {i}");
        }
    }

    #[test]
    fn dry_mix_passes_input_through() {
        let mut ph = Phaser::new(0.5, 0.7, 0.5, 0.0, 44_100.0);
        let sr = 44_100.0_f32;
        for i in 0..1024 {
            let input = (TAU * 440.0 * i as f32 / sr).sin();
            let out = ph.process_sample(input);
            assert!((out - input).abs() < 1e-6, "mix=0 should be dry");
        }
    }

    #[test]
    fn output_bounded_with_max_feedback() {
        // tanh saturation guarantees feedback bounded.
        let mut ph = Phaser::new(0.5, 1.0, 0.95, 1.0, 44_100.0);
        for i in 0..44_100 {
            let input = ((i as f32 * 0.1).sin()).clamp(-1.0, 1.0);
            let out = ph.process_sample(input);
            assert!(out.abs() < 5.0, "diverged: {out} at {i}");
        }
    }

    #[test]
    fn shape_sweep_fixed_points() {
        // Endpoints should still be 0 and 1.
        assert!((Phaser::shape_sweep(0.0)).abs() < 0.01);
        assert!((Phaser::shape_sweep(1.0) - 1.0).abs() < 0.01);
        // Midpoint should be near 0.5 too.
        let mid = Phaser::shape_sweep(0.5);
        assert!((mid - 0.5).abs() < 0.05, "mid: {mid}");
    }
}
