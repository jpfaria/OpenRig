//! Transformer saturation — output transformer (OT) saturation as found
//! in tube guitar / hi-fi amplifiers. Symmetric core saturation produces
//! predominantly odd harmonics with a strong 3rd-harmonic component;
//! eddy-current losses in the laminated core roll off the upper highs.
//!
//! References:
//! - Foley, R. (2008). 'Investigation into Audio Output Transformer
//!   Saturation', AES Convention paper 7392.
//! - Karjalainen, M. & Pakarinen, J. (2006). 'Wave Digital Simulation of
//!   a Vacuum-Tube Amplifier', ICASSP 2006 — chapter on output stage.
//!
//! Topology:
//! - Input HPF (DC block + low-freq cut to mimic OT primary inductance).
//! - 2× polyphase oversampling around the nonlinearity.
//! - Symmetric soft clip = tanh(x) with an added cubic 3rd-harmonic
//!   emphasis (transformer cores produce proportionally more 3rd
//!   harmonic than tanh alone).
//! - Output LPF mimicking eddy-current losses (~6 kHz roll-off).
//! - Output level.

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

pub const MODEL_ID: &str = "transformer_saturation";
pub const DISPLAY_NAME: &str = "Transformer Saturation";
const BRAND: &str = block_core::BRAND_NATIVE;

#[derive(Debug, Clone, Copy)]
struct Settings {
    drive: f32,
    /// Strength of the cubic 3rd-harmonic emphasis (0..1).
    color: f32,
    /// Output LPF emulating core eddy losses; lower = darker.
    warmth: f32,
    level: f32,
}

struct TransformerProcessor {
    settings: Settings,
    in_hpf: OnePoleHighPass,
    eddy_lpf: OnePoleLowPass,
    upsample_lpf: OnePoleLowPass,
    downsample_lpf: OnePoleLowPass,
}

impl TransformerProcessor {
    fn new(settings: Settings, sample_rate: f32) -> Self {
        let oversample_rate = sample_rate * 2.0;
        Self {
            settings,
            in_hpf: OnePoleHighPass::new(40.0, sample_rate),
            // Default eddy roll-off ~ 6 kHz; modulated by `warmth`.
            eddy_lpf: OnePoleLowPass::new(6_000.0, sample_rate),
            upsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
            downsample_lpf: OnePoleLowPass::new(sample_rate * 0.45, oversample_rate),
        }
    }

    fn pct(v: f32) -> f32 { (v / 100.0).clamp(0.0, 1.0) }

    /// Symmetric saturator with optional 3rd-harmonic emphasis.
    /// `color` 0 → pure tanh; `color` 1 → tanh + cubic boost weighted to
    /// emphasise the 3rd harmonic content the way an iron-core OT does.
    #[inline]
    fn shape(x: f32, color: f32) -> f32 {
        let base = x.tanh();
        // Cubic shaper component: sin³(ωt) = (3·sin - sin(3ωt))/4 — the
        // -sin(3ωt) term is the added 3rd harmonic. We use clip² as a
        // proxy because tanh saturates the input first.
        let cubic_boost = base * base * base;
        // Mix: at color=1 we add 30% of the cubic boost, which is enough
        // to sound transformer-y without overwhelming the fundamental.
        base + 0.3 * color * cubic_boost
    }
}

impl MonoProcessor for TransformerProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let drive = Self::pct(self.settings.drive);
        let color = Self::pct(self.settings.color);
        let warmth = Self::pct(self.settings.warmth);
        let level = Self::pct(self.settings.level);

        let x = self.in_hpf.process(input);
        // Drive: 0 → unity (no sat), 100 → ~10× (heavy sat).
        let driven = x * (1.0 + drive * 9.0);

        // 2× polyphase oversampling around the nonlinearity.
        let up0 = self.upsample_lpf.process(driven * 2.0);
        let up1 = self.upsample_lpf.process(0.0);
        let s0 = Self::shape(up0, color);
        let s1 = Self::shape(up1, color);
        let _ = self.downsample_lpf.process(s0);
        let down = self.downsample_lpf.process(s1);

        // Eddy-current LPF: warmth=1 fully open, warmth=0 darkest.
        let warm = self.eddy_lpf.process(down);
        let toned = down * warmth + warm * (1.0 - warmth);

        toned * (level * 1.5)  // 50% = 0.75x — drive raises perceived level
    }
}

struct DualMonoProcessor { left: TransformerProcessor, right: TransformerProcessor }

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
            float_parameter("drive", "Drive", Some("Gain"), Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("color", "Color", Some("Character"), Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("warmth", "Warmth", Some("EQ"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("level", "Level", Some("Output"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn read_settings(p: &ParameterSet) -> Result<Settings> {
    Ok(Settings {
        drive: required_f32(p, "drive").map_err(anyhow::Error::msg)?,
        color: required_f32(p, "color").map_err(anyhow::Error::msg)?,
        warmth: required_f32(p, "warmth").map_err(anyhow::Error::msg)?,
        level: required_f32(p, "level").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(p: &ParameterSet) -> Result<()> { let _ = read_settings(p)?; Ok(()) }
pub fn asset_summary(_: &ParameterSet) -> Result<String> {
    Ok("native='transformer_saturation' algorithm='tanh + cubic 3rd-harmonic 2x oversampled'".to_string())
}
fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(p: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let s = read_settings(p)?;
    Ok(match layout {
        AudioChannelLayout::Mono => BlockProcessor::Mono(Box::new(TransformerProcessor::new(s, sample_rate))),
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: TransformerProcessor::new(s, sample_rate),
            right: TransformerProcessor::new(s, sample_rate),
        })),
    })
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Native,
    schema, validate: validate_params, asset_summary, build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Settings {
        Settings { drive: 40.0, color: 60.0, warmth: 50.0, level: 50.0 }
    }

    #[test]
    fn shape_silence_in_silence_out() {
        for c in [0.0_f32, 0.3, 0.7, 1.0] {
            assert!(TransformerProcessor::shape(0.0, c).abs() < 1e-9);
        }
    }

    #[test]
    fn shape_is_symmetric_around_zero() {
        // Transformer saturation should be odd-symmetric: shape(-x) == -shape(x).
        for c in [0.0_f32, 0.5, 1.0] {
            for x in [0.1_f32, 0.5, 1.0, 5.0] {
                let p = TransformerProcessor::shape(x, c);
                let n = TransformerProcessor::shape(-x, c);
                assert!((p + n).abs() < 1e-6, "shape({x}, {c})={p} vs shape(-{x},{c})={n}");
            }
        }
    }

    #[test]
    fn shape_is_bounded() {
        for c in [0.0_f32, 1.0] {
            for x in [-100.0_f32, -10.0, 10.0, 100.0] {
                let y = TransformerProcessor::shape(x, c);
                // tanh + 0.3 * tanh^3 → max output ~1.3 at extreme drive.
                assert!(y.abs() <= 1.5, "shape({x}, {c}) = {y}");
            }
        }
    }

    #[test]
    fn silence_input_produces_silence() {
        let mut p = TransformerProcessor::new(defaults(), 44_100.0);
        for _ in 0..2048 {
            assert!(p.process_sample(0.0).abs() < 1e-3);
        }
    }

    #[test]
    fn sine_input_finite_and_nonzero() {
        let mut p = TransformerProcessor::new(defaults(), 44_100.0);
        let sr = 44_100.0_f32;
        let mut any_nonzero = false;
        for i in 0..2048 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.4;
            let out = p.process_sample(s);
            assert!(out.is_finite());
            if out.abs() > 1e-6 { any_nonzero = true; }
        }
        assert!(any_nonzero);
    }
}
