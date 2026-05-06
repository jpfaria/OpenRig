//! Pitch-tracking wah — sweeps the wah filter according to the
//! spectral centroid of the input. Bright notes pull the cutoff up,
//! dark notes let it fall.
//!
//! Reference: spectral-centroid pitch following has a long history in
//! analog "smart wah" pedals (Morley Smart Box, EBS BassIQ "Track"
//! mode). Cheaper than a true pitch tracker (YIN / MPM) and good
//! enough for a wah-style modulator.
//!
//! Pro-tier topology:
//!   1. Linkwitz-style high/low split via SVF at the centroid pivot
//!      (~700 Hz). The high band is the "treble" energy, the low band
//!      the "bass" energy.
//!   2. Two EnvelopeFollowers (one per band) with the same attack/
//!      release. Output ratio `hp / (lp + hp)` is the centroid index
//!      in [0, 1].
//!   3. The ratio drives a second SVF (the wah filter) sweeping
//!      cutoff between min and max. Q is a knob.
//!   4. DcBlocker on the wet sum.
//!
//! Knobs:
//!   * Sensitivity — input gain into the centroid envelopes.
//!   * Range — how far the wah sweeps.
//!   * Q — resonance.
//!   * Mix.
//!
//! RT-safe: 2 SVFs, 2 EnvelopeFollowers, 1 DcBlocker, no allocation.

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

pub const MODEL_ID: &str = "pitch_wah";
pub const DISPLAY_NAME: &str = "Pitch-Tracking Wah";

const SPLIT_HZ: f32 = 700.0;
const MIN_CUTOFF_HZ: f32 = 250.0;
const MAX_CUTOFF_HZ: f32 = 2_400.0;

#[derive(Clone, Copy)]
pub struct PitchWahParams {
    pub sensitivity: f32, // [0.25, 4]
    pub range: f32,       // [0, 1]
    pub q: f32,           // [0.5, 12]
    pub mix: f32,
}

pub struct PitchWah {
    splitter: Svf,
    wah: Svf,
    lp_env: EnvelopeFollower,
    hp_env: EnvelopeFollower,
    dc_blocker: DcBlocker,
    sensitivity: f32,
    min_cutoff_hz: f32,
    max_cutoff_hz: f32,
    q: f32,
    mix: f32,
}

impl PitchWah {
    pub fn new(p: PitchWahParams, sample_rate: f32) -> Self {
        let span = (MAX_CUTOFF_HZ - MIN_CUTOFF_HZ) * p.range.clamp(0.0, 1.0);
        Self {
            splitter: Svf::new(SPLIT_HZ, 0.7, sample_rate),
            wah: Svf::new(MIN_CUTOFF_HZ, p.q, sample_rate),
            // Slow-ish envelope so the centroid index doesn't jitter
            // every cycle of a low note.
            lp_env: EnvelopeFollower::from_ms(15.0, 80.0, sample_rate),
            hp_env: EnvelopeFollower::from_ms(15.0, 80.0, sample_rate),
            dc_blocker: DcBlocker::new(5.0, sample_rate),
            sensitivity: p.sensitivity,
            min_cutoff_hz: MIN_CUTOFF_HZ,
            max_cutoff_hz: MIN_CUTOFF_HZ + span,
            q: p.q,
            mix: p.mix.clamp(0.0, 1.0),
        }
    }
}

impl MonoProcessor for PitchWah {
    fn process_sample(&mut self, input: f32) -> f32 {
        let scaled = input * self.sensitivity;
        let frame = self.splitter.process(scaled);
        let lp_e = self.lp_env.process(frame.low);
        let hp_e = self.hp_env.process(frame.high);
        let total = lp_e + hp_e + 1.0e-9;
        let centroid = (hp_e / total).clamp(0.0, 1.0);
        let cutoff = self.min_cutoff_hz + (self.max_cutoff_hz - self.min_cutoff_hz) * centroid;
        self.wah.set_cutoff_q(cutoff, self.q);
        let bp = self.wah.process_band(input);
        let wet = self.dc_blocker.process(flush_denormal(bp));
        (1.0 - self.mix) * input + self.mix * wet
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "wah".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("sensitivity", "Sensitivity", Some("Wah"), Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("range", "Range", Some("Wah"), Some(80.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("q", "Q", Some("Wah"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", Some("Output"), Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    })
}

fn parse(params: &ParameterSet) -> Result<PitchWahParams> {
    Ok(PitchWahParams {
        sensitivity: 0.25 + (required_f32(params, "sensitivity").map_err(Error::msg)? / 100.0) * 3.75,
        range: required_f32(params, "range").map_err(Error::msg)? / 100.0,
        q: 0.5 + (required_f32(params, "q").map_err(Error::msg)? / 100.0) * 11.5,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

fn validate(params: &ParameterSet) -> Result<()> { let _ = parse(params)?; Ok(()) }

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let p = parse(params)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(PitchWah::new(p, sample_rate)))),
        AudioChannelLayout::Stereo => {
            struct Dual { l: Box<dyn MonoProcessor>, r: Box<dyn MonoProcessor> }
            impl StereoProcessor for Dual {
                fn process_frame(&mut self, i: [f32; 2]) -> [f32; 2] {
                    [self.l.process_sample(i[0]), self.r.process_sample(i[1])]
                }
            }
            Ok(BlockProcessor::Stereo(Box::new(Dual {
                l: Box::new(PitchWah::new(p, sample_rate)),
                r: Box::new(PitchWah::new(p, sample_rate)),
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
#[path = "native_pitch_wah_tests.rs"]
mod tests;
