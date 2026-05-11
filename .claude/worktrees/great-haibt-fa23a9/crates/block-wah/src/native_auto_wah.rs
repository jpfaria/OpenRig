//! Auto-wah — envelope-driven band-pass filter. Pro-tier.
//!
//! Reference: Mu-Tron III (Musitronics, 1972) and Q-Tron envelope
//! filter circuits — both follow the input level with an envelope
//! detector and use it to sweep the centre frequency of a resonant
//! filter. The classic "quack" comes from the combination of a high-Q
//! band-pass and an attack-fast/release-slow envelope.
//!
//! Pro-tier topology:
//!   1. EnvelopeFollower (peak detector with separate attack/release
//!      time-constants) — taken from `block_core::dsp::legacy`.
//!   2. State-Variable Filter (Simper 2011 ZDF) — sweep-friendly,
//!      preserves resonance under fast modulation.
//!   3. DcBlocker post-bandpass — keeps long sustains from drifting
//!      asymmetric envelope DC into the chain.
//!   4. Anti-denormal flush on the wet sample.
//!
//! Voicing knobs (sensitivity, attack, release, range, Q, mix) are
//! exposed via `AutoWahTuning`. New variants live in
//! `native_auto_wah_*.rs` and instantiate `AutoWah::with_tuning(...)`.
//!
//! User-facing knobs stay constant across variants:
//!   * Sensitivity (%) — how hot the envelope reads
//!   * Range (%) — how far the cutoff sweeps
//!   * Q (%) — resonance
//!   * Mix (%) — dry/wet
//!
//! RT-safe: zero alloc on hot path, EnvelopeFollower + Svf state is
//! plain f32 fields.

use anyhow::{Error, Result};
use block_core::dsp::{flush_denormal, DcBlocker, EnvelopeFollower, Svf};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};

use crate::registry::WahModelDefinition;
use crate::WahBackendKind;

pub const MODEL_ID: &str = "auto_wah_classic";
pub const DISPLAY_NAME: &str = "Auto-Wah (Classic)";

/// Hidden voicing of an auto-wah variant. Variants in
/// `native_auto_wah_*.rs` instantiate `AutoWah::with_tuning(...)`.
#[derive(Debug, Clone, Copy)]
pub struct AutoWahTuning {
    /// Filter cutoff at envelope = 0 (idle).
    pub min_cutoff_hz: f32,
    /// Filter cutoff at envelope ≈ 1 (peak).
    pub max_cutoff_hz: f32,
    /// Filter resonance (Q). Higher = peakier "vowel" sound.
    pub q: f32,
    /// Envelope-follower attack time-constant.
    pub attack_ms: f32,
    /// Envelope-follower release time-constant.
    pub release_ms: f32,
    /// Input pre-gain into the envelope detector — sets sensitivity.
    pub sensitivity: f32,
}

impl AutoWahTuning {
    /// Mu-Tron III voice — moderate range, mid-Q, mid attack/release.
    pub const CLASSIC: Self = Self {
        min_cutoff_hz: 250.0,
        max_cutoff_hz: 2_000.0,
        q: 6.0,
        attack_ms: 5.0,
        release_ms: 80.0,
        sensitivity: 1.0,
    };
}

#[derive(Clone, Copy)]
pub struct AutoWahParams {
    pub sensitivity: f32, // [0, 4]
    pub range: f32,       // [0, 1] scales sweep span
    pub q: f32,           // [0.5, 12]
    pub mix: f32,         // [0, 1]
}

pub struct AutoWah {
    svf: Svf,
    env: EnvelopeFollower,
    dc_blocker: DcBlocker,
    min_cutoff_hz: f32,
    max_cutoff_hz: f32,
    q: f32,
    sensitivity: f32,
    mix: f32,
}

impl AutoWah {
    pub fn new(p: AutoWahParams, sample_rate: f32) -> Self {
        Self::with_tuning(p, sample_rate, AutoWahTuning::CLASSIC)
    }

    pub fn with_tuning(p: AutoWahParams, sample_rate: f32, tuning: AutoWahTuning) -> Self {
        // User Range scales the swept span around min cutoff.
        let span = (tuning.max_cutoff_hz - tuning.min_cutoff_hz) * p.range.clamp(0.0, 1.0);
        let max_cutoff_hz = tuning.min_cutoff_hz + span;
        // Combine the user Q knob with the variant-default Q.
        let q = (tuning.q * p.q.max(0.05)).clamp(0.3, 30.0);
        // Compose user sensitivity with variant default.
        let sensitivity = (tuning.sensitivity * p.sensitivity.max(0.0)).clamp(0.0, 8.0);

        Self {
            svf: Svf::new(tuning.min_cutoff_hz, q, sample_rate),
            env: EnvelopeFollower::from_ms(tuning.attack_ms, tuning.release_ms, sample_rate),
            dc_blocker: DcBlocker::new(5.0, sample_rate),
            min_cutoff_hz: tuning.min_cutoff_hz,
            max_cutoff_hz,
            q,
            sensitivity,
            mix: p.mix.clamp(0.0, 1.0),
        }
    }
}

impl MonoProcessor for AutoWah {
    fn process_sample(&mut self, input: f32) -> f32 {
        let env = self.env.process(input * self.sensitivity).min(1.0);
        let cutoff = self.min_cutoff_hz + (self.max_cutoff_hz - self.min_cutoff_hz) * env;
        self.svf.set_cutoff_q(cutoff, self.q);
        let bp = self.svf.process_band(input);
        let wet = self.dc_blocker.process(flush_denormal(bp));
        (1.0 - self.mix) * input + self.mix * wet
    }
}

pub fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "wah".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "sensitivity",
                "Sensitivity",
                Some("Wah"),
                Some(60.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "range",
                "Range",
                Some("Wah"),
                Some(70.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "q",
                "Q",
                Some("Wah"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Output"),
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    })
}

pub fn params_from_set(params: &ParameterSet) -> Result<AutoWahParams> {
    let sensitivity_pct = required_f32(params, "sensitivity").map_err(Error::msg)?;
    let range_pct = required_f32(params, "range").map_err(Error::msg)?;
    let q_pct = required_f32(params, "q").map_err(Error::msg)?;
    let mix_pct = required_f32(params, "mix").map_err(Error::msg)?;
    Ok(AutoWahParams {
        sensitivity: 0.25 + (sensitivity_pct / 100.0) * 3.75, // [0.25, 4]
        range: range_pct / 100.0,
        q: 0.3 + (q_pct / 100.0) * 1.7, // [0.3, 2] multiplier on tuning.q
        mix: mix_pct / 100.0,
    })
}

fn validate(params: &ParameterSet) -> Result<()> {
    let _ = params_from_set(params)?;
    Ok(())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let p = params_from_set(params)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(AutoWah::new(
            p,
            sample_rate,
        )))),
        AudioChannelLayout::Stereo => {
            struct DualMono {
                left: Box<dyn MonoProcessor>,
                right: Box<dyn MonoProcessor>,
            }
            impl StereoProcessor for DualMono {
                fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
                    [
                        self.left.process_sample(input[0]),
                        self.right.process_sample(input[1]),
                    ]
                }
            }
            Ok(BlockProcessor::Stereo(Box::new(DualMono {
                left: Box::new(AutoWah::new(p, sample_rate)),
                right: Box::new(AutoWah::new(p, sample_rate)),
            })))
        }
    }
}

pub const MODEL_DEFINITION: WahModelDefinition = WahModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: WahBackendKind::Native,
    schema,
    validate,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

#[cfg(test)]
#[path = "native_auto_wah_tests.rs"]
mod tests;
