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

/// Voicing of a phaser variant — number of all-pass sections, sweep
/// frequency range, and JFET-skew strength. Variants live in
/// `native_phaser_*.rs` and instantiate `Phaser::with_tuning(...)`.
#[derive(Debug, Clone, Copy)]
pub struct PhaserTuning {
    /// Number of cascaded all-pass sections (more = sharper notches,
    /// more midrange "shimmer").
    pub stages: usize,
    /// Bottom of the LFO sweep in Hz.
    pub min_freq_hz: f32,
    /// Top of the LFO sweep in Hz.
    pub max_freq_hz: f32,
    /// Strength of the JFET sweep skew. 0 = pure log-linear sweep,
    /// 1 = full tanh skew. Default voice mixes 0.7 shaped + 0.3
    /// linear; variants tune this knob to taste.
    pub skew_strength: f32,
}

impl PhaserTuning {
    /// 6-stage, 200–2000 Hz, 70 % JFET skew. The "MXR Phase 90" voice.
    pub const CLASSIC: Self = Self {
        stages: 6,
        min_freq_hz: 200.0,
        max_freq_hz: 2_000.0,
        skew_strength: 0.7,
    };
}

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
    stage_z: Vec<f32>,
    feedback_z: f32,
    feedback_dc_blocker: DcBlocker,
    skew_strength: f32,
    log_min: f32,
    log_max: f32,
}

impl Phaser {
    pub fn new(rate_hz: f32, depth: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        Self::with_tuning(
            rate_hz,
            depth,
            feedback,
            mix,
            sample_rate,
            PhaserTuning::CLASSIC,
        )
    }

    pub fn with_tuning(
        rate_hz: f32,
        depth: f32,
        feedback: f32,
        mix: f32,
        sample_rate: f32,
        tuning: PhaserTuning,
    ) -> Self {
        Self {
            depth: depth.clamp(0.0, 1.0),
            feedback: feedback.clamp(-0.95, 0.95),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            lfo: Lfo::new(LfoShape::Sine, rate_hz, sample_rate),
            stage_z: vec![0.0; tuning.stages.max(1)],
            feedback_z: 0.0,
            feedback_dc_blocker: DcBlocker::new(5.0, sample_rate),
            skew_strength: tuning.skew_strength.clamp(0.0, 1.0),
            log_min: tuning.min_freq_hz.ln(),
            log_max: tuning.max_freq_hz.ln(),
        }
    }

    pub fn set_lfo_phase(&mut self, phase: f32) {
        self.lfo.set_phase(phase);
    }

    /// JFET-like sweep skew. `t` in [0, 1] (raw LFO unipolar). Output
    /// in [0, 1] but biased toward the upper portion of the sweep,
    /// emulating the exponential V→R curve of the 2N5952 in pinch-off.
    /// Per-instance skew strength so each variant can dial in a taste.
    fn shape_sweep(&self, t: f32) -> f32 {
        const K: f32 = 2.5;
        let centered = t * 2.0 - 1.0;
        let denom = K.tanh();
        let shaped = (centered * K).tanh() / denom * 0.5 + 0.5;
        let s = self.skew_strength;
        s * shaped + (1.0 - s) * t
    }
}

impl MonoProcessor for Phaser {
    fn process_sample(&mut self, input: f32) -> f32 {
        let lfo_raw = self.lfo.next_unipolar();
        let lfo_shaped = self.shape_sweep(lfo_raw);

        // Sweep break frequency on the per-instance log range, JFET-shaped.
        let log_freq = self.log_min + (self.log_max - self.log_min) * self.depth * lfo_shaped;
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
#[path = "native_phaser_tests.rs"]
mod tests;
