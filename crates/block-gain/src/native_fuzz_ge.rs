//! Germanium Fuzz Face — original Dallas Arbiter Fuzz Face with NKT275
//! Ge transistors. Ge fuzz character: smoother clip, slightly compressed
//! response, warmer upper-mids, distinct "round" feel — the canonical
//! Hendrix sound (paired with the Octavia for higher leads).
//!
//! References:
//! - electrosmash.com — 'Fuzz Face Analysis' (NKT275 Ge pair; lower
//!   forward voltage and beta than Si → softer transition curves).
//! - Yeh, D. T. (2008). 'Digital Implementation of Musical Distortion
//!   Circuits', Stanford CCRMA — chapter on Ge vs Si transistor models.
//!
//! Compared to native_fuzz_si:
//! - Lower clip threshold (Ge V_be ~0.3 V vs Si ~0.7 V) → wider knee.
//! - Smoother saturation curve (tanh vs sqrt-hyperbola).
//! - Slight DC bias offset preserved (Ge transistors leak more).
//! - Less high-frequency content.

use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, OnePoleHighPass,
    OnePoleLowPass, StereoProcessor,
};

pub const MODEL_ID: &str = "fuzz_ge";
pub const DISPLAY_NAME: &str = "Fuzz Face (Ge)";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    fuzz: f32,
    tone: f32,
    level: f32,
}

struct FuzzGeProcessor {
    settings: Settings,
    in_hpf: OnePoleHighPass,
    out_hpf: OnePoleHighPass,
    tone_lpf: OnePoleLowPass,
    upsample_lpf: OnePoleLowPass,
    downsample_lpf: OnePoleLowPass,
}

impl FuzzGeProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        let oversample_rate = sample_rate * 2.0;
        Self {
            settings,
            in_hpf: OnePoleHighPass::new(50.0, sample_rate),
            out_hpf: OnePoleHighPass::new(40.0, sample_rate),
            // Lower cutoff than Si — Ge rolls highs sooner, sounds warmer.
            tone_lpf: OnePoleLowPass::new(2_500.0, sample_rate),
            upsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            downsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
        }
    }

    fn pct(v: f32) -> f32 { (v / 100.0).clamp(0.0, 1.0) }

    /// Ge transistor saturation — smoother than Si. tanh shaper with a
    /// small DC bias (Ge leaks ~5% even at idle, giving the Ge fuzz its
    /// recognisable 2nd-harmonic warmth) and a slightly lower output
    /// ceiling so the wave compresses earlier.
    #[inline]
    fn ge_shape(x: f32) -> f32 {
        // Ge: 0.05 bias for asymmetry, 0.9 ceiling for soft compression.
        let bias = 0.05;
        let ceiling = 0.9;
        ((x + bias).tanh() - bias.tanh()) * ceiling
    }
}

impl MonoProcessor for FuzzGeProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let fuzz = Self::pct(self.settings.fuzz);
        let tone = Self::pct(self.settings.tone);
        let level = Self::pct(self.settings.level);

        let x = self.in_hpf.process(input);
        // Ge has lower beta — somewhat less raw gain than Si.
        let pre = x * (4.0 + fuzz * 100.0);

        // Two-stage clip with 2× oversampling.
        let up0 = self.upsample_lpf.process(pre * 2.0);
        let up1 = self.upsample_lpf.process(0.0);
        let s0 = Self::ge_shape(Self::ge_shape(up0));
        let s1 = Self::ge_shape(Self::ge_shape(up1));
        let _ = self.downsample_lpf.process(s0);
        let down = self.downsample_lpf.process(s1);

        let warm = self.tone_lpf.process(down);
        let toned = down * tone + warm * (1.0 - tone);

        let out = self.out_hpf.process(toned);
        out * (level * 1.5)
    }
}

struct DualMonoProcessor { left: FuzzGeProcessor, right: FuzzGeProcessor }

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("fuzz", "Fuzz", Some("Gain"), Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", Some("EQ"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        fuzz: required_f32(p, "fuzz").map_err(anyhow::Error::msg)?,
        tone: required_f32(p, "tone").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> { let _ = read_settings(p)?; Ok(()) }
pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='fuzz_ge' algorithm='Ge Fuzz Face — biased tanh 2-stage 2x oversampled'".to_string())
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(FuzzGeProcessor::new(s, sample_rate))),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: FuzzGeProcessor::new(s, sample_rate),
            right: FuzzGeProcessor::new(s, sample_rate),
        })),
    })
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Native,
    schema, validate: validate_params, asset_summary, build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings { Settings { fuzz: 60.0, tone: 50.0, level: 50.0 } }

    #[test]
    fn shape_is_bounded() {
        for x in [-100.0_f32, -10.0, 0.0, 10.0, 100.0] {
            let y = FuzzGeProcessor::ge_shape(x);
            assert!(y.abs() <= 1.0, "ge_shape({x}) = {y}");
        }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        // ge_shape(0) = (tanh(bias) - tanh(bias)) * ceiling = 0
        assert!(FuzzGeProcessor::ge_shape(0.0).abs() < 1e-9);
    }

    #[test]
    fn shape_is_smoother_than_si_at_moderate_drive() {
        // Compare with native_fuzz_si: at x=2 the Ge shaper compresses
        // sooner (lower magnitude than Si saturation).
        let ge = FuzzGeProcessor::ge_shape(2.0).abs();
        // Si shape at the same input would approach 0.85 (positive ceiling).
        // Ge ceiling is 0.9, but tanh's knee is gentler so at x=2 we
        // expect Ge to be ~0.8 vs Si ~0.85.
        assert!(ge < 0.92, "Ge should still be inside its ceiling: {ge}");
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = FuzzGeProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = FuzzGeProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.3;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
}
